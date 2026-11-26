//! Moving pixels between the CPU and textures. Used by native front ends
//! and by the web export's read-back of transparent frames (row 97).

use crate::{GpuContext, GpuError};

const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

impl GpuContext {
    /// Bytes per pixel of the context's texture format (always 4 here).
    pub fn bytes_per_pixel(&self) -> u32 {
        4
    }

    /// Whether CPU-side pixels for this context are ordered B, G, R, A
    /// (the native format) rather than R, G, B, A (the WebGL fallback).
    pub fn pixels_are_bgra(&self) -> bool {
        matches!(
            self.texture_format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        )
    }

    /// Creates a texture the compositor can sample, filled from tightly
    /// packed 8-bit pixels in the context's channel order.
    pub fn upload_texture(
        &self,
        width: u32,
        height: u32,
        pixels: &[u8],
        label: &'static str,
    ) -> wgpu::Texture {
        let texture = self.create_render_texture(width, height, label);
        self.write_texture(&texture, width, height, pixels);
        texture
    }

    /// Replaces the contents of a texture of exactly `width` x `height`.
    pub fn write_texture(&self, texture: &wgpu::Texture, width: u32, height: u32, pixels: &[u8]) {
        debug_assert_eq!(pixels.len(), (width * height * self.bytes_per_pixel()) as usize);
        self.queue().write_texture(
            texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * self.bytes_per_pixel()),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Copies a texture back to the CPU as tightly packed 8-bit pixels in
    /// the context's channel order. Blocks until the GPU has finished.
    pub fn read_texture(&self, texture: &wgpu::Texture) -> Result<Vec<u8>, GpuError> {
        let readback = self.begin_read_texture(texture);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
        self.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| GpuError::Readback("the device stopped responding".to_owned()))?;
        receiver
            .recv()
            .map_err(|_| GpuError::Readback("the map callback never ran".to_owned()))?
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        Ok(readback.pixels())
    }

    /// Submits a copy of `texture` into a buffer the CPU can map. The
    /// caller maps [`TextureReadback::buffer`] for reading (waiting however
    /// its platform waits: a blocking poll natively, the event loop on the
    /// web) and then takes [`TextureReadback::pixels`].
    pub fn begin_read_texture(&self, texture: &wgpu::Texture) -> TextureReadback {
        let width = texture.width();
        let height = texture.height();
        let unpadded_row = width * self.bytes_per_pixel();
        let padded_row = unpadded_row.div_ceil(COPY_BYTES_PER_ROW_ALIGNMENT)
            * COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-readback-buffer"),
            size: (padded_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-readback-encoder"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue().submit([encoder.finish()]);
        TextureReadback { buffer, height, padded_row, unpadded_row }
    }
}

/// A texture copied into a mappable buffer ([`GpuContext::begin_read_texture`]).
pub struct TextureReadback {
    pub buffer: wgpu::Buffer,
    height: u32,
    padded_row: u32,
    unpadded_row: u32,
}

impl TextureReadback {
    /// The tightly packed pixels, once `buffer` is mapped for reading; unmaps it.
    pub fn pixels(&self) -> Vec<u8> {
        let mapped = self.buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((self.unpadded_row * self.height) as usize);
        for row in 0..self.height as usize {
            let start = row * self.padded_row as usize;
            pixels.extend_from_slice(&mapped[start..start + self.unpadded_row as usize]);
        }
        drop(mapped);
        self.buffer.unmap();
        pixels
    }
}

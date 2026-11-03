use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};

/// Wraps tightly packed BGRA pixels (what the renderer and decoders
/// produce) as a GPUI image. GPUI stores images in BGRA, so no conversion.
pub(crate) fn image_from_bgra(width: u32, height: u32, bgra: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::smallvec![Frame::new(
        buffer
    )])))
}

/// Box-filter downscale of a BGRA image to `target_width` (keeping aspect).
pub(crate) fn downscale_bgra(
    width: u32,
    height: u32,
    bgra: &[u8],
    target_width: u32,
) -> (u32, u32, Vec<u8>) {
    if width <= target_width || width == 0 || height == 0 {
        return (width, height, bgra.to_vec());
    }
    let target_width = target_width.max(1);
    let target_height = ((height as f64 * target_width as f64 / width as f64).round() as u32).max(1);
    let mut out = vec![0u8; (target_width * target_height * 4) as usize];
    for ty in 0..target_height {
        let y0 = (ty as u64 * height as u64 / target_height as u64) as u32;
        let y1 = (((ty + 1) as u64 * height as u64 / target_height as u64) as u32).max(y0 + 1);
        for tx in 0..target_width {
            let x0 = (tx as u64 * width as u64 / target_width as u64) as u32;
            let x1 = (((tx + 1) as u64 * width as u64 / target_width as u64) as u32).max(x0 + 1);
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    let index = ((y * width + x) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += bgra[index + channel] as u64;
                    }
                    count += 1;
                }
            }
            let index = ((ty * target_width + tx) * 4) as usize;
            for channel in 0..4 {
                out[index + channel] = (sum[channel] / count.max(1)) as u8;
            }
        }
    }
    (target_width, target_height, out)
}

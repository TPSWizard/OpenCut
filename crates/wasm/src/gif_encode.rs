//! GIF export for the web editor (row 96): the shared `gifenc` crate, so
//! the web writes the same palette, frames and bytes as the desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// The GIF size for a canvas and a wanted width: `[width, height]`.
#[wasm_bindgen(js_name = gifSize)]
pub fn gif_size(canvas_width: u32, canvas_height: u32, width: u32) -> Vec<u32> {
    let (width, height) = ::gifenc::gif_size(canvas_width, canvas_height, width);
    vec![width, height]
}

/// Frame delays in centiseconds for `count` frames at `fps`.
#[wasm_bindgen(js_name = gifFrameDelays)]
pub fn gif_frame_delays(count: u32, fps: f64) -> Vec<u16> {
    ::gifenc::frame_delays(count as usize, fps)
}

/// Colours counted from sampled frames, for the palette.
#[wasm_bindgen]
pub struct GifPalette(::gifenc::Histogram);

#[wasm_bindgen]
impl GifPalette {
    #[wasm_bindgen(constructor)]
    pub fn new() -> GifPalette {
        Self(::gifenc::Histogram::new())
    }

    /// Counts an RGBA frame.
    #[wasm_bindgen(js_name = addRgba)]
    pub fn add_rgba(&mut self, rgba: &[u8]) {
        self.0.add_rgba(rgba);
    }

    /// Up to `colours` colours as flat RGB bytes.
    pub fn colours(&self, colours: u32) -> Vec<u8> {
        self.0.palette(colours as usize).into_iter().flatten().collect()
    }
}

impl Default for GifPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// A looping GIF written frame by frame.
#[wasm_bindgen]
pub struct GifEncoder(Option<::gifenc::GifWriter>);

#[wasm_bindgen]
impl GifEncoder {
    /// `palette` is flat RGB bytes (from `GifPalette.colours`).
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, palette: &[u8], dither: bool) -> GifEncoder {
        let colours = palette.chunks_exact(3).map(|rgb| [rgb[0], rgb[1], rgb[2]]).collect();
        Self(Some(::gifenc::GifWriter::new(width, height, colours, dither)))
    }

    /// Adds an RGBA frame shown for `delay` centiseconds.
    #[wasm_bindgen(js_name = addRgba)]
    pub fn add_rgba(&mut self, rgba: &[u8], delay: u16) {
        if let Some(writer) = &mut self.0 {
            writer.add_rgba(rgba, delay);
        }
    }

    /// The finished GIF (empty when already finished).
    pub fn finish(&mut self) -> Vec<u8> {
        self.0.take().map(::gifenc::GifWriter::finish).unwrap_or_default()
    }
}

//! Video scopes for the web editor: the shared `scopes` crate, so the web
//! draws the waveform, RGB parade, vectorscope and histogram of its preview
//! exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

/// A drawn scope: an RGBA picture and what it measured.
#[wasm_bindgen]
pub struct VideoScope(::scopes::ScopeImage);

#[wasm_bindgen]
impl VideoScope {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.0.width as u32
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.0.height as u32
    }

    /// The picture, `width` × `height` RGBA pixels (a copy).
    #[wasm_bindgen(getter)]
    pub fn rgba(&self) -> Vec<u8> {
        self.0.rgba.clone()
    }

    /// Samples read from the frame.
    #[wasm_bindgen(getter)]
    pub fn samples(&self) -> u32 {
        self.0.stats.samples as u32
    }

    /// Share of samples (0 to 1) with a channel at 0.
    #[wasm_bindgen(getter, js_name = atBlack)]
    pub fn at_black(&self) -> f64 {
        self.0.stats.at_black
    }

    /// Share of samples with a channel at 255.
    #[wasm_bindgen(getter, js_name = atWhite)]
    pub fn at_white(&self) -> f64 {
        self.0.stats.at_white
    }

    /// Luma the darkest 1% of samples sit at or below (0 to 255).
    #[wasm_bindgen(getter, js_name = blackLevel)]
    pub fn black_level(&self) -> u32 {
        self.0.stats.black_level as u32
    }

    /// Luma the brightest 1% sit at or above.
    #[wasm_bindgen(getter, js_name = whiteLevel)]
    pub fn white_level(&self) -> u32 {
        self.0.stats.white_level as u32
    }
}

/// Draws a scope (`waveform`, `parade`, `vectorscope` or `histogram`) of a
/// `width` × `height` RGBA frame.
#[wasm_bindgen(js_name = drawVideoScope)]
pub fn draw_video_scope(scope: &str, rgba: &[u8], width: u32, height: u32) -> Result<VideoScope, String> {
    let scope = ::scopes::Scope::parse(scope).ok_or_else(|| format!("Unknown scope: {scope}"))?;
    Ok(VideoScope(::scopes::draw(scope, rgba, width as usize, height as usize, ::scopes::PixelOrder::Rgba)))
}

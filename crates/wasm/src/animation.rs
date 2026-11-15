#![cfg(target_arch = "wasm32")]

//! Lottie animations for the web editor, drawn by the same Rust code as the
//! desktop's (`crates/lottie`).

use wasm_bindgen::prelude::*;

/// A parsed, checked Lottie or dotLottie file that draws its frames as RGBA.
#[wasm_bindgen(js_name = LottieAnimation)]
pub struct LottieAnimation {
    inner: ::lottie::Animation,
}

#[wasm_bindgen(js_class = LottieAnimation)]
impl LottieAnimation {
    /// Reads a Lottie JSON file or a dotLottie archive; throws with the
    /// reason when it is not one or is outside the limits.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<LottieAnimation, JsError> {
        ::lottie::Animation::from_bytes(bytes)
            .map(|inner| Self { inner })
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.inner.info().width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.inner.info().height
    }

    #[wasm_bindgen(getter, js_name = frameRate)]
    pub fn frame_rate(&self) -> f64 {
        self.inner.info().frame_rate
    }

    #[wasm_bindgen(getter, js_name = inFrame)]
    pub fn in_frame(&self) -> f64 {
        self.inner.info().in_frame
    }

    #[wasm_bindgen(getter, js_name = outFrame)]
    pub fn out_frame(&self) -> f64 {
        self.inner.info().out_frame
    }

    #[wasm_bindgen(getter, js_name = durationSeconds)]
    pub fn duration_seconds(&self) -> f64 {
        self.inner.info().duration_seconds()
    }

    /// The Lottie frame shown `seconds` into the clip (it loops).
    #[wasm_bindgen(js_name = frameAt)]
    pub fn frame_at(&self, seconds: f64) -> f64 {
        self.inner.info().frame_at(seconds)
    }

    /// Straight-alpha RGBA of `frame` drawn at `width` × `height`.
    #[wasm_bindgen(js_name = renderRgba)]
    pub fn render_rgba(&mut self, frame: f64, width: u32, height: u32) -> Vec<u8> {
        self.inner.render_rgba(frame, width, height)
    }
}

/// Whether `bytes` look like a Lottie JSON file or a dotLottie archive.
#[wasm_bindgen(js_name = isLottie)]
pub fn is_lottie(bytes: &[u8]) -> bool {
    ::lottie::is_lottie(bytes)
}

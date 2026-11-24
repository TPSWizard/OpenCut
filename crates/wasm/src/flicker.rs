//! Deflicker for the web editor: the shared `deflicker` crate, so the web
//! measures frames and evens out flicker exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

/// The mean Rec. 709 luma (0-255) of an RGBA frame.
#[wasm_bindgen(js_name = deflickerFrameLevel)]
pub fn deflicker_frame_level(rgba: &[u8], width: u32, height: u32) -> f64 {
    ::deflicker::frame_level_rgba(rgba, width as usize, height as usize)
}

/// Frames averaged for a strength (`off`, `light`, `medium`, `strong`);
/// 0 when off or unknown.
#[wasm_bindgen(js_name = deflickerWindow)]
pub fn deflicker_window(strength: &str) -> u32 {
    ::deflicker::Strength::parse(strength).map_or(0, |strength| strength.window() as u32)
}

/// The levels as the clip stores them (`deflicker.levels`).
#[wasm_bindgen(js_name = deflickerEncodeLevels)]
pub fn deflicker_encode_levels(media_id: &str, start: f64, rate: f64, values: Vec<f64>) -> String {
    ::deflicker::Levels { media_id: media_id.to_owned(), start, rate, values }.encode()
}

/// A clip's stored levels, parsed once.
#[wasm_bindgen]
pub struct DeflickerLevels(::deflicker::Levels);

#[wasm_bindgen]
impl DeflickerLevels {
    /// Parses `deflicker.levels`; throws when it is not valid.
    #[wasm_bindgen(constructor)]
    pub fn new(text: &str) -> Result<DeflickerLevels, String> {
        ::deflicker::Levels::parse(text).map(Self).ok_or_else(|| "The stored deflicker levels are not valid".to_owned())
    }

    #[wasm_bindgen(js_name = mediaId)]
    pub fn media_id(&self) -> String {
        self.0.media_id.clone()
    }

    pub fn frames(&self) -> u32 {
        self.0.values.len() as u32
    }

    /// The brightness gain at a source time with `window` frames averaged.
    #[wasm_bindgen(js_name = gainAt)]
    pub fn gain_at(&self, source_seconds: f64, window: u32) -> f64 {
        self.0.gain_at(source_seconds, window as usize)
    }
}

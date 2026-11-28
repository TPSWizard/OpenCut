//! Stabilise for the web editor (row 107): the shared `stabilise` crate, so
//! the web measures the camera path and corrects each frame exactly as the
//! desktop editor does (`editor::stabilise_correction`, `steady_transform`).

use std::collections::HashMap;

use stabilise::{CameraPath, Corrections, Similarity, Strength};
use wasm_bindgen::prelude::*;

/// The measured picture's width and height for a source frame size.
#[wasm_bindgen(js_name = stabiliseMeasureSize)]
pub fn stabilise_measure_size(width: u32, height: u32) -> Vec<u32> {
    let (w, h) = stabilise::measure_size(width, height);
    vec![w, h]
}

/// Frames measured a second for a file's frame rate (0 or NaN when unknown).
#[wasm_bindgen(js_name = stabiliseMeasureRate)]
pub fn stabilise_measure_rate(fps: f64) -> f64 {
    stabilise::measure_rate((fps.is_finite() && fps > 0.0).then_some(fps))
}

/// Whether `text` is a strength (`off`, `light`, `medium`, `strong`, `still`).
#[wasm_bindgen(js_name = stabiliseIsStrength)]
pub fn stabilise_is_strength(text: &str) -> bool {
    Strength::parse(text).is_some()
}

/// The camera's movement from RGBA frame `p` to `q`: `[a, b, tx, ty]` in
/// frame units, or empty when it cannot be measured.
#[wasm_bindgen(js_name = stabiliseStep)]
pub fn stabilise_step(p: &[u8], q: &[u8], width: u32, height: u32) -> Vec<f64> {
    stabilise::frame_step(p, q, width as usize, height as usize).map_or_else(Vec::new, |s| vec![s.a, s.b, s.tx, s.ty])
}

/// The path as the clip stores it (`stabilise.path`); `steps` holds four
/// numbers a step, NaN for a step not measured.
#[wasm_bindgen(js_name = stabiliseEncodePath)]
pub fn stabilise_encode_path(media_id: &str, start: f64, rate: f64, aspect: f64, steps: Vec<f64>) -> String {
    let steps = steps
        .chunks_exact(4)
        .map(|c| c.iter().all(|v| v.is_finite()).then(|| Similarity { a: c[0], b: c[1], tx: c[2], ty: c[3] }))
        .collect();
    CameraPath { media_id: media_id.to_owned(), start, rate, aspect, steps }.encode()
}

/// Where the corrected frame is drawn: `[localX, localY, turnDegrees,
/// scale]` for a frame drawn `width_x` / `width_y` wide along each axis
/// (`stabilise::drawn_offset`).
#[wasm_bindgen(js_name = stabiliseDrawnOffset)]
#[allow(clippy::too_many_arguments)]
pub fn stabilise_drawn_offset(a: f64, b: f64, tx: f64, ty: f64, width_x: f64, width_y: f64, flip_x: bool, flip_y: bool) -> Vec<f64> {
    let (x, y, turn, scale) = stabilise::drawn_offset(&Similarity { a, b, tx, ty }, width_x, width_y, flip_x, flip_y);
    vec![x, y, turn, scale]
}

/// A clip's stored camera path, parsed once, with its corrections worked
/// out once per strength.
#[wasm_bindgen]
pub struct StabilisePath {
    path: CameraPath,
    corrections: HashMap<Strength, Corrections>,
}

#[wasm_bindgen]
impl StabilisePath {
    /// Parses `stabilise.path`; throws when it is not valid.
    #[wasm_bindgen(constructor)]
    pub fn new(text: &str) -> Result<StabilisePath, String> {
        CameraPath::parse(text)
            .map(|path| Self { path, corrections: HashMap::new() })
            .ok_or_else(|| "The stored camera path is not valid".to_owned())
    }

    #[wasm_bindgen(js_name = mediaId)]
    pub fn media_id(&self) -> String {
        self.path.media_id.clone()
    }

    pub fn frames(&self) -> u32 {
        self.path.frames() as u32
    }

    fn corrections_for(&mut self, strength: &str) -> &Corrections {
        let strength = Strength::parse(strength).unwrap_or(Strength::Off);
        let path = &self.path;
        self.corrections.entry(strength).or_insert_with(|| path.corrections(strength))
    }

    /// `[zoom, needed]`: the zoom used for `strength`, and the zoom that
    /// would hide every edge (above the first, edges show at times).
    pub fn zoom(&mut self, strength: &str) -> Vec<f64> {
        let corrections = self.corrections_for(strength);
        vec![corrections.zoom, corrections.needed]
    }

    /// The frame's correction `[a, b, tx, ty]` at a source time.
    #[wasm_bindgen(js_name = correctionAt)]
    pub fn correction_at(&mut self, strength: &str, source_seconds: f64) -> Vec<f64> {
        let s = self.corrections_for(strength).at(source_seconds);
        vec![s.a, s.b, s.tx, s.ty]
    }
}

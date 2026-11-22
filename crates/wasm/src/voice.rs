//! Voice effects for the web editor: the shared `voice` crate, one channel
//! per call, so the web and the desktop change voices identically.

use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// One channel through a voice effect (`deeper`, `higher`, `chipmunk`,
/// `monster`, `robot`, `echo`, `radio`, `hall`); same length, not normalised.
#[wasm_bindgen(js_name = applyVoiceEffect)]
pub fn apply_voice_effect(effect: &str, samples: &[f32], sample_rate: f32) -> Result<Vec<f32>, JsError> {
    voice::apply_voice_effect(effect, samples, sample_rate).ok_or_else(|| JsError::new(&format!("unknown voice effect {effect}")))
}

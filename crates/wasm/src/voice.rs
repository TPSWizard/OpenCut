//! Voice effects for the web editor: the shared `voice` crate, one channel
//! per call, so the web and the desktop change voices identically.

use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// One channel through a voice effect (`deeper`, `higher`, `chipmunk`,
/// `monster`, `robot`, `echo`, `radio`, `hall`); same length, not normalised.
#[wasm_bindgen(js_name = applyVoiceEffect)]
pub fn apply_voice_effect(effect: &str, samples: &[f32], sample_rate: f32) -> Result<Vec<f32>, JsError> {
    voice::apply_voice_effect(effect, samples, sample_rate).ok_or_else(|| JsError::new(&format!("unknown voice effect {effect}")))
}

/// A clip's channels after [`enhance_voice`], with the loudness before and after.
#[wasm_bindgen]
pub struct EnhancedVoice {
    samples: Vec<f32>,
    report: voice::EnhanceReport,
}

#[wasm_bindgen]
impl EnhancedVoice {
    /// The channels, interleaved as they were passed in.
    #[wasm_bindgen(getter)]
    pub fn samples(&self) -> Vec<f32> {
        self.samples.clone()
    }

    #[wasm_bindgen(getter, js_name = inputLufs)]
    pub fn input_lufs(&self) -> f64 {
        self.report.input_lufs
    }

    #[wasm_bindgen(getter, js_name = outputLufs)]
    pub fn output_lufs(&self) -> f64 {
        self.report.output_lufs
    }

    #[wasm_bindgen(getter, js_name = noiseReduced)]
    pub fn noise_reduced(&self) -> bool {
        self.report.noise_reduced
    }
}

/// Enhance voice (high-pass, noise reduction, EQ, de-esser, compressor,
/// −16 LUFS) over a clip's `channels` interleaved channels, as the desktop does.
#[wasm_bindgen(js_name = enhanceVoice)]
pub fn enhance_voice(interleaved: &[f32], channels: usize, sample_rate: f32) -> Result<EnhancedVoice, JsError> {
    if channels == 0 || interleaved.len() % channels != 0 {
        return Err(JsError::new("enhanceVoice needs whole frames of at least one channel"));
    }
    let mut planes: Vec<Vec<f32>> = (0..channels).map(|channel| interleaved.iter().skip(channel).step_by(channels).copied().collect()).collect();
    let report = voice::enhance_voice(&mut planes, sample_rate);
    let frames = interleaved.len() / channels;
    let mut samples = Vec::with_capacity(interleaved.len());
    for frame in 0..frames {
        for plane in &planes {
            samples.push(plane[frame]);
        }
    }
    Ok(EnhancedVoice { samples, report })
}

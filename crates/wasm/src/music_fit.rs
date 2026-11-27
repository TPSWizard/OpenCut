//! Fit music to a length for the web editor (row 101): the shared
//! `musicfit` crate finds the beats and the bar lines that sound alike and
//! lays out the pieces, the same as the desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// The settings: JSON `{sampleRate, crossfadeTicks, minSeconds, maxSeconds, maxPieces}`.
#[wasm_bindgen(js_name = musicFitConstants)]
pub fn music_fit_constants() -> String {
    serde_json::json!({
        "sampleRate": ::musicfit::SAMPLE_RATE,
        "crossfadeTicks": ::musicfit::CROSSFADE_TICKS,
        "minSeconds": ::musicfit::MIN_TARGET_SECONDS,
        "maxSeconds": ::musicfit::MAX_TARGET_SECONDS,
        "maxPieces": ::musicfit::MAX_PIECES,
    })
    .to_string()
}

/// Fits a clip (`{start, duration, trimStart, trimEnd, fadeIn, fadeOut}`,
/// ticks and seconds) whose playing span `samples` holds (mono at
/// `sample_rate`) to `target` ticks: JSON `{kind, pieces: [{start,
/// duration, trimStart, trimEnd, fadeIn, fadeOut, crossfade}], jumps,
/// bpm, trimmed}`, or the reason it cannot as the error.
#[wasm_bindgen(js_name = musicFit)]
pub fn music_fit(samples: &[f32], sample_rate: u32, clip_json: &str, target: f64) -> Result<String, String> {
    let clip: ::musicfit::Clip = serde_json::from_str(clip_json).map_err(|error| error.to_string())?;
    let fit = ::musicfit::fit(samples, sample_rate, &clip, target.round() as i64).map_err(|error| error.message().to_owned())?;
    serde_json::to_string(&fit).map_err(|error| error.to_string())
}

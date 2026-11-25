//! Audio crossfades for the web editor: the shared `crossfade` crate, so
//! the web joins neighbouring clips exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

fn clips(json: &str) -> Result<Vec<::crossfade::TrackClip>, String> {
    serde_json::from_str(json).map_err(|error| error.to_string())
}

/// A track's clips (JSON `[{ start, duration, trimStart, trimEnd, retimed,
/// eligible, fadeIn, fadeOut, crossfade, curve }]`, ticks) as they play
/// with their crossfades: JSON, one entry per clip, `null` for a clip no
/// crossfade touches, else `{ start, duration, trimStart, trimEnd, fadeIn,
/// fadeOut, fadeInCurve, fadeOutCurve }`.
#[wasm_bindgen(js_name = audioCrossfades)]
pub fn audio_crossfades(clips_json: &str) -> Result<String, String> {
    serde_json::to_string(&::crossfade::crossfade_track(&clips(clips_json)?)).map_err(|error| error.to_string())
}

/// The one-step crossfade of `ticks` from clip `index` into the next:
/// JSON `{ ok: plan }` (`{ to, length, trimOut, trimIn, shift, fromEdit,
/// toEdit, moved }`) or `{ error, message }`.
#[wasm_bindgen(js_name = planCrossfade)]
pub fn plan_crossfade(clips_json: &str, index: usize, ticks: f64) -> Result<String, String> {
    let clips = clips(clips_json)?;
    let value = match ::crossfade::plan(&clips, index, ticks.round() as i64) {
        Ok(plan) => serde_json::json!({ "ok": plan }),
        Err(error) => serde_json::json!({ "error": error, "message": error.message() }),
    };
    Ok(value.to_string())
}

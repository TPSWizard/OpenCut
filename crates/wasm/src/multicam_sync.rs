//! Multicam for the web editor: the shared `multicam` crate, so the web
//! lines angles up by their sound and turns cut lists into opacity keys
//! exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

/// The onset envelope of mono `samples` at `rate` (`multicam::onsets`).
#[wasm_bindgen(js_name = multicamOnsets)]
pub fn multicam_onsets(samples: &[f32], rate: f64) -> Vec<f64> {
    ::multicam::onsets(samples, rate)
}

/// `[lag, score]` lining `other`'s onsets up with `reference`'s within
/// `max_lag` seconds; empty when they cannot be compared.
#[wasm_bindgen(js_name = multicamSyncLag)]
pub fn multicam_sync_lag(reference: &[f64], other: &[f64], max_lag: f64) -> Vec<f64> {
    ::multicam::sync_lag(reference, other, max_lag).map_or_else(Vec::new, |result| vec![result.lag, result.score])
}

/// Below this score two angles' sound does not match well enough to sync.
#[wasm_bindgen(js_name = multicamMinSyncScore)]
pub fn multicam_min_sync_score() -> f64 {
    ::multicam::MIN_SYNC_SCORE
}

/// The stored cut list with `angle` from timeline second `time` on.
#[wasm_bindgen(js_name = multicamAddCut)]
pub fn multicam_add_cut(cuts: &str, time: f64, angle: u32) -> String {
    ::multicam::encode_cuts(&::multicam::add_cut(&::multicam::parse_cuts(cuts), time, angle as usize))
}

/// How many cuts a stored cut list holds.
#[wasm_bindgen(js_name = multicamCutCount)]
pub fn multicam_cut_count(cuts: &str) -> u32 {
    ::multicam::parse_cuts(cuts).len() as u32
}

/// The opacity keys of `angle`'s clip over timeline seconds `start..end`
/// for a stored cut list, flat `[localSeconds, value, …]`.
#[wasm_bindgen(js_name = multicamOpacityKeys)]
pub fn multicam_opacity_keys(cuts: &str, angle: u32, start: f64, end: f64) -> Vec<f64> {
    ::multicam::opacity_keys(&::multicam::parse_cuts(cuts), angle as usize, start, end)
        .into_iter()
        .flat_map(|(time, value)| [time, value])
        .collect()
}

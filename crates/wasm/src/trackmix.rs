//! Track mixing for the web editor: the shared `trackmix` crate, so mute,
//! solo, track volume and pan work out the same as on the desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// How each track plays: JSON `[{ muted, solo, volume, pan, sounding }]` in,
/// JSON `[{ audible, gain, pan }]` out, in the same order.
#[wasm_bindgen(js_name = trackMix)]
pub fn track_mix(tracks: &str) -> Result<String, String> {
    let tracks: Vec<::trackmix::TrackSettings> = serde_json::from_str(tracks).map_err(|error| error.to_string())?;
    serde_json::to_string(&::trackmix::track_mix(&tracks)).map_err(|error| error.to_string())
}

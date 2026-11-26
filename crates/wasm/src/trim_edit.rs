//! Slip, slide and roll for the web editor: the shared `trimedit` crate,
//! so both editors make the same edit with the same limits.

use wasm_bindgen::prelude::wasm_bindgen;

/// The edit a trim tool (`slip`, `slide`, `roll`) makes on `target` (an id)
/// among its track's clips (JSON `[{id, start, duration, trimStart,
/// trimEnd, limited, retimed}]`, ticks) moved by `delta` ticks: JSON
/// `{delta, changes: [{id, start, duration, trimStart, trimEnd}]}`.
/// Throws with the reason when the edit cannot be made.
#[wasm_bindgen(js_name = trimEditPlan)]
pub fn trim_edit_plan(tool: &str, target: &str, track: &str, delta: f64, min_duration: f64) -> Result<String, String> {
    let tool = ::trimedit::Tool::parse(tool).ok_or("The tool is slip, slide or roll")?;
    let clips: Vec<::trimedit::Clip> = serde_json::from_str(track).map_err(|error| error.to_string())?;
    let clip = clips.iter().find(|clip| clip.id == target).ok_or("The clip is not on its track")?;
    let plan = ::trimedit::plan(tool, clip, &clips, delta.round() as i64, min_duration.round().max(1.0) as i64)?;
    serde_json::to_string(&plan).map_err(|error| error.to_string())
}

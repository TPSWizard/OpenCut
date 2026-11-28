//! Project interchange for the web editor (row 111): the shared
//! `interchange` crate, so the web writes and reads OpenTimelineIO, FCPXML
//! and CMX 3600 EDL exactly as the desktop editor does. Timelines cross as
//! the crate's JSON (`interchange::Timeline`).

use interchange::{Format, Rate, Timeline};
use wasm_bindgen::prelude::*;

fn format(name: &str) -> Result<Format, String> {
    Format::parse(name).ok_or_else(|| format!("Unknown timeline format {name:?} (otio, fcpxml or edl)"))
}

fn timeline(json: &str) -> Result<Timeline, String> {
    serde_json::from_str(json).map_err(|error| format!("Not a timeline: {error}"))
}

/// `timeline` (JSON) written as `format` (`otio`, `fcpxml`, `edl`).
#[wasm_bindgen(js_name = interchangeWrite)]
pub fn interchange_write(timeline_json: &str, format_name: &str) -> Result<String, String> {
    Ok(interchange::write(&timeline(timeline_json)?, format(format_name)?))
}

/// What `format` cannot hold of `timeline`, as a JSON array of sentences.
#[wasm_bindgen(js_name = interchangeFormatNotes)]
pub fn interchange_format_notes(timeline_json: &str, format_name: &str) -> Result<String, String> {
    let notes = interchange::format_notes(&timeline(timeline_json)?, format(format_name)?);
    Ok(serde_json::to_string(&notes).unwrap_or_default())
}

/// The format of a file by its name or text (`otio`, `fcpxml`, `edl`), or empty.
#[wasm_bindgen(js_name = interchangeFormatOf)]
pub fn interchange_format_of(file_name: &str, text: &str) -> String {
    Format::sniff(file_name, text).map_or(String::new(), |f| f.extension().to_owned())
}

/// Reads a timeline file; an EDL is read at `num`/`den` frames a second.
/// Returns the timeline as JSON.
#[wasm_bindgen(js_name = interchangeRead)]
pub fn interchange_read(text: &str, file_name: &str, num: u32, den: u32) -> Result<String, String> {
    let format = Format::sniff(file_name, text).ok_or("That is not an OpenTimelineIO, FCPXML or EDL file")?;
    let timeline = interchange::read(text, format, Rate::new(num.max(1), den.max(1)))?;
    serde_json::to_string(&timeline).map_err(|error| error.to_string())
}

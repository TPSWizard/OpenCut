//! Subtitle import for the web editor: the shared `subtitleparse` crate,
//! so SRT and WebVTT files read the same in both editors.

use wasm_bindgen::prelude::wasm_bindgen;

/// Parses an `.srt` or `.vtt` file's text (the format from `file_name`,
/// sniffed from the text otherwise): JSON `{cues: [{text, start, end,
/// words?}], skipped}`, times in seconds.
#[wasm_bindgen(js_name = parseSubtitles)]
pub fn parse_subtitles(input: &str, file_name: &str) -> String {
    let parsed = ::subtitleparse::parse(input, ::subtitleparse::Format::from_name(file_name));
    serde_json::to_string(&parsed).unwrap_or_else(|_| "{\"cues\":[],\"skipped\":0}".to_owned())
}

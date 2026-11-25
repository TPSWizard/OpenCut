//! Retakes for the web editor: the shared `retakes` crate, so the web finds
//! the same earlier takes and false starts in a transcript as the desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// The retakes among timed words (JSON `[{ text, start, end }]`), with
/// options (JSON, camelCase, missing ones at their defaults): JSON
/// `[{ kind, words, kept, similarity, start, end, text, keptText }]`.
#[wasm_bindgen(js_name = findRetakes)]
pub fn find_retakes(words: &str, options: &str) -> Result<String, String> {
    let words: Vec<::retakes::Word> = serde_json::from_str(words).map_err(|error| error.to_string())?;
    let options: ::retakes::RetakeOptions =
        if options.trim().is_empty() { Default::default() } else { serde_json::from_str(options).map_err(|error| error.to_string())? };
    serde_json::to_string(&::retakes::find_retakes(&words, &options)).map_err(|error| error.to_string())
}

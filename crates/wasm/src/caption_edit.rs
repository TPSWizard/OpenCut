//! Caption find and replace, highlight and auto emoji for the web editor:
//! the shared `captionedit` crate, so both editors rewrite captions alike.

use wasm_bindgen::prelude::wasm_bindgen;

fn read(captions: &str) -> Result<Vec<::captionedit::Caption>, String> {
    serde_json::from_str(captions).map_err(|error| error.to_string())
}

/// How many places `query` matches in `captions` (JSON `[{id, words: [{text,
/// start, end, keyword}]}]`, times in ticks).
#[wasm_bindgen(js_name = captionFindCount)]
pub fn caption_find_count(captions: &str, query: &str) -> Result<u32, String> {
    Ok(::captionedit::find(&read(captions)?, query).len() as u32)
}

/// A caption edit (`replace`, `highlight`, `addEmoji`, `removeEmoji`):
/// JSON `[{id, content, words}]` for the captions that changed.
#[wasm_bindgen(js_name = captionEdit)]
pub fn caption_edit(op: &str, captions: &str, query: &str, replacement: &str) -> Result<String, String> {
    let captions = read(captions)?;
    let edited = match op {
        "replace" => ::captionedit::replace(&captions, query, replacement),
        "highlight" => ::captionedit::highlight(&captions, query),
        "addEmoji" => ::captionedit::add_emoji(&captions),
        "removeEmoji" => ::captionedit::remove_emoji(&captions),
        other => return Err(format!("Unknown caption edit: {other}")),
    };
    serde_json::to_string(&edited).map_err(|error| error.to_string())
}

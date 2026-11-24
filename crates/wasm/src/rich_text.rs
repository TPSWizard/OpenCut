//! Rich text for the web editor: the shared `richtext` crate, so the web
//! resolves, moves and edits styled ranges exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

/// Each character's style for `text` (JSON array of `{color?, bold?,
/// italic?, scale?}`) from the stored `richText` JSON; empty when nothing
/// is styled or the JSON is not valid.
#[wasm_bindgen(js_name = richTextStyles)]
pub fn rich_text_styles(stored: &str, text: &str) -> String {
    ::richtext::styles_for(stored, text)
        .map(|styles| serde_json::to_string(&styles).unwrap_or_default())
        .unwrap_or_default()
}

/// The `richText` JSON after setting `patch` (JSON `{color?, bold?,
/// italic?, scale?}`) on characters `start..end` of `text`.
#[wasm_bindgen(js_name = richTextStyleRange)]
pub fn rich_text_style_range(stored: &str, text: &str, start: u32, end: u32, patch: &str) -> Result<String, String> {
    let patch: ::richtext::Patch = serde_json::from_str(patch).map_err(|error| error.to_string())?;
    let rich = ::richtext::style_range(::richtext::parse(stored).as_ref(), text, start as usize, end as usize, &patch);
    Ok(::richtext::encode(&rich))
}

/// The `richText` JSON after putting characters `start..end` of `text`
/// back to the clip's own style.
#[wasm_bindgen(js_name = richTextClearRange)]
pub fn rich_text_clear_range(stored: &str, text: &str, start: u32, end: u32) -> String {
    ::richtext::encode(&::richtext::clear_range(::richtext::parse(stored).as_ref(), text, start as usize, end as usize))
}

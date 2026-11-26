//! Word art for the web editor (row 99): the shared `wordart` crate styles
//! the canvas glyphs (gradient fill, stacked outlines, inner shadow) the
//! same way the desktop renderer styles its glyph raster, and holds the
//! style gallery.

use wasm_bindgen::prelude::wasm_bindgen;

/// The style gallery: JSON `[{id, label, description, params}]`.
#[wasm_bindgen(js_name = wordArtStyles)]
pub fn word_art_styles() -> String {
    serde_json::to_string(::wordart::styles()).unwrap_or_else(|_| "[]".to_owned())
}

/// The params a gallery style sets on a text clip (every word-art param,
/// so a previous style is cleared): a JSON object, or `null` for an
/// unknown id.
#[wasm_bindgen(js_name = wordArtStyleParams)]
pub fn word_art_style_params(id: &str) -> String {
    match ::wordart::style_params(id) {
        Some(values) => serde_json::Value::Object(values.into_iter().collect()).to_string(),
        None => "null".to_owned(),
    }
}

/// Whole pixels a glyph raster needs around the glyphs for the outlines of
/// the style in `params_json` (a JSON object of the clip's params), for a
/// font `em_px` pixels tall; 0 for plain text.
#[wasm_bindgen(js_name = wordArtOutset)]
pub fn word_art_outset(params_json: &str, em_px: f64) -> u32 {
    ::wordart::WordArt::from_params_json(params_json).map_or(0, |art| art.outset_px(em_px))
}

/// Styles a glyph raster (straight-alpha RGBA, glyphs only) with the style
/// in `params_json`; the same size back, or empty for plain text.
#[wasm_bindgen(js_name = wordArtCompose)]
pub fn word_art_compose(params_json: &str, width: u32, height: u32, rgba: &[u8], em_px: f64) -> Vec<u8> {
    match ::wordart::WordArt::from_params_json(params_json) {
        Some(art) => art.compose(width as usize, height as usize, rgba, em_px),
        None => Vec::new(),
    }
}

//! Curved text for the web editor: the shared `textwarp` crate, so the web
//! and the desktop bend text identically.

use wasm_bindgen::prelude::wasm_bindgen;

/// The size `[width, height]` of a `width` × `height` text raster bent by
/// `curve` (-100 to 100), centred on the flat raster's centre; the flat
/// size when the curve is flat.
#[wasm_bindgen(js_name = bentTextSize)]
pub fn bent_text_size(curve: f64, width: u32, height: u32) -> Vec<u32> {
    let (width, height) = ::textwarp::bent_size(curve, width, height);
    vec![width, height]
}

/// The text raster (straight-alpha RGBA) bent by `curve`, at
/// `bentTextSize`; empty when the curve is flat (draw it as it is).
#[wasm_bindgen(js_name = bendText)]
pub fn bend_text(curve: f64, width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    ::textwarp::warp(curve, width, height, rgba).map(|bent| bent.pixels).unwrap_or_default()
}

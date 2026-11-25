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

/// The control points of a text path (`textPath` preset, `textPathAmount`,
/// `textPathPoints`), flattened `[x0, y0, x1, y1, ...]`; empty for none.
#[wasm_bindgen(js_name = textPathPoints)]
pub fn text_path_points(preset: &str, amount: f64, custom: &str) -> Vec<f64> {
    ::textwarp::read_path(preset, Some(amount), custom).map(|points| points.into_iter().flatten().collect()).unwrap_or_default()
}

fn pairs(points: &[f64]) -> Vec<[f64; 2]> {
    points.chunks_exact(2).map(|pair| [pair[0], pair[1]]).collect()
}

/// The size `[width, height]` of a text raster laid along a path.
#[wasm_bindgen(js_name = textPathSize)]
pub fn text_path_size(points: &[f64], width: u32, height: u32) -> Vec<u32> {
    let (width, height) = ::textwarp::path_size(&pairs(points), width, height);
    vec![width, height]
}

/// The text raster (straight-alpha RGBA) laid along a path, at
/// `textPathSize`; empty when the path has no length.
#[wasm_bindgen(js_name = bendTextAlongPath)]
pub fn bend_text_along_path(points: &[f64], width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    ::textwarp::warp_path(&pairs(points), width, height, rgba).map(|bent| bent.pixels).unwrap_or_default()
}

/// The text raster (straight-alpha RGBA) bent by `curve`, at
/// `bentTextSize`; empty when the curve is flat (draw it as it is).
#[wasm_bindgen(js_name = bendText)]
pub fn bend_text(curve: f64, width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    ::textwarp::warp(curve, width, height, rgba).map(|bent| bent.pixels).unwrap_or_default()
}

//! Freehand drawing for the web editor: the shared `strokes` crate, so a
//! drawing's strokes are timed, stored and rasterised exactly as on the
//! desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// A drawing (`strokes` as stored) as `width` x `height` straight-alpha
/// RGBA, drawn on up to `progress` (0 to 1). Empty when it does not read.
#[wasm_bindgen(js_name = rasteriseDrawing)]
pub fn rasterise_drawing(strokes: &str, width: u32, height: u32, progress: f64) -> Vec<u8> {
    match ::strokes::Drawing::parse(strokes) {
        Some(drawing) => ::strokes::rasterise(&drawing, width as usize, height as usize, progress),
        None => Vec::new(),
    }
}

/// How far drawing on has got (0 to 1) `elapsed` seconds into the element.
#[wasm_bindgen(js_name = drawingRevealProgress)]
pub fn drawing_reveal_progress(draw_on: f64, elapsed: f64) -> f64 {
    ::strokes::reveal_progress(draw_on, elapsed)
}

/// The size a drawing is rasterised at for a canvas, `[width, height]`.
#[wasm_bindgen(js_name = drawingRasterSize)]
pub fn drawing_raster_size(canvas_width: u32, canvas_height: u32) -> Vec<u32> {
    let (width, height) = ::strokes::raster_size(canvas_width, canvas_height, ::strokes::MAX_RASTER_SIDE);
    vec![width, height]
}

/// The drawing with one more stroke (JSON `{tool, color, width, points:
/// [[x, y, pressure, time], ...]}`), timed after the others.
#[wasm_bindgen(js_name = drawingAddStroke)]
pub fn drawing_add_stroke(strokes: &str, stroke: &str) -> Result<String, String> {
    let mut drawing = ::strokes::Drawing::parse(strokes).ok_or("The drawing's strokes do not read")?;
    let stroke: ::strokes::Stroke = serde_json::from_str(stroke).map_err(|error| format!("The stroke does not read: {error}"))?;
    if stroke.points.is_empty() {
        return Err("A stroke needs at least one point".to_owned());
    }
    // Parse once more so widths and colours are checked as stored ones are.
    let checked = ::strokes::Drawing::parse(&::strokes::Drawing { strokes: vec![stroke] }.encode()).ok_or("The stroke does not read")?;
    drawing.push_stroke(checked.strokes.into_iter().next().expect("one stroke"));
    Ok(drawing.encode())
}

/// The drawing without its last stroke.
#[wasm_bindgen(js_name = drawingRemoveLastStroke)]
pub fn drawing_remove_last_stroke(strokes: &str) -> String {
    let mut drawing = ::strokes::Drawing::parse(strokes).unwrap_or_default();
    drawing.strokes.pop();
    drawing.encode()
}

/// How many strokes a drawing has (0 when it does not read).
#[wasm_bindgen(js_name = drawingStrokeCount)]
pub fn drawing_stroke_count(strokes: &str) -> u32 {
    ::strokes::Drawing::parse(strokes).map_or(0, |drawing| drawing.strokes.len() as u32)
}

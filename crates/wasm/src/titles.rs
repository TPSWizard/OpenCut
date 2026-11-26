//! Titles for the web editor (row 98): the shared `titles` crate's credit
//! roll positions and text template library, so both editors scroll
//! credits and place templates the same way.

use wasm_bindgen::prelude::wasm_bindgen;

/// The text template library as written (`{categories, templates}`).
#[wasm_bindgen(js_name = textTemplateLibrary)]
pub fn text_template_library() -> String {
    ::titles::LIBRARY_JSON.to_owned()
}

/// The template's text clips placed for a canvas: JSON `[{name, start,
/// duration, params}]` (seconds; complete params with content, position
/// and font size), or `null` for an unknown id.
#[wasm_bindgen(js_name = placeTextTemplate)]
pub fn place_text_template(id: &str, canvas_width: u32, canvas_height: u32) -> String {
    match ::titles::text_template(id) {
        Some(template) => serde_json::to_string(&::titles::place_text_template(template, canvas_width, canvas_height))
            .unwrap_or_else(|_| "null".to_owned()),
        None => "null".to_owned(),
    }
}

/// The clip's position along a credit roll (`direction` "up": its
/// positionY; "left": its positionX) `local_seconds` into a clip
/// `clip_seconds` long, from the text's drawn box relative to its anchor
/// (canvas pixels after scale). `speed` 0 fits the clip. NaN for a
/// direction that does not roll.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = creditRollPosition)]
pub fn credit_roll_position(
    direction: &str,
    speed: f64,
    local_seconds: f64,
    clip_seconds: f64,
    canvas_width: f64,
    canvas_height: f64,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> f64 {
    match ::titles::credit_roll_from(direction, speed) {
        Some(roll) => ::titles::credit_roll_position(
            roll,
            local_seconds,
            clip_seconds,
            canvas_width,
            canvas_height,
            ::titles::RollBox { left, top, width, height },
        ),
        None => f64::NAN,
    }
}

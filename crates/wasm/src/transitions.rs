#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

/// The transitions the compositor can draw, as `TransitionInfo[]`: name,
/// author, licence, description, tags and parameters (name, GLSL kind,
/// default, min, max, step, color, description). See
/// `crates/compositor/src/transitions.rs`.
#[wasm_bindgen(js_name = listTransitions)]
pub fn list_transitions() -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&compositor::transition_catalogue())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

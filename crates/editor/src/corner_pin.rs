//! Corner pin on a video, picture or sticker clip, from its params:
//! `cornerPin.enabled` and `cornerPin.<corner><X|Y>` for the corners
//! `topLeft`, `topRight`, `bottomRight` and `bottomLeft`, in percent of the
//! clip's own box before its rotation (0, 0 is its top left; 100, 100 its
//! bottom right), so moving, scaling or turning the clip carries the pin.
//! The corners are keyframable. The shared compositor draws the pin
//! (`compositor::pin_matrix`); the web editor's `@/corner-pin/pin` reads the
//! same params. Checked against `fixtures/corner-pin/cases.json`.

use crate::Params;

pub const CORNER_PIN_MIN: f64 = -500.0;
pub const CORNER_PIN_MAX: f64 = 600.0;
pub const CORNER_PIN_ENABLED: &str = "cornerPin.enabled";
/// The corners in drawing order, with their default places in percent.
pub const CORNER_PIN_CORNERS: [(&str, [f64; 2]); 4] = [
    ("topLeft", [0.0, 0.0]),
    ("topRight", [100.0, 0.0]),
    ("bottomRight", [100.0, 100.0]),
    ("bottomLeft", [0.0, 100.0]),
];

/// The param key for one corner's axis, e.g. `cornerPin.topLeftX`.
pub fn corner_pin_key(corner: &str, axis: char) -> String {
    format!("cornerPin.{corner}{axis}")
}

/// Every corner param key, X then Y for each corner in order.
pub fn corner_pin_keys() -> Vec<String> {
    CORNER_PIN_CORNERS
        .iter()
        .flat_map(|(corner, _)| [corner_pin_key(corner, 'X'), corner_pin_key(corner, 'Y')])
        .collect()
}

/// The corners in box units (1 = the whole box), whether or not the pin is
/// on; values that are not numbers fall back to the defaults.
pub fn corner_pin_corners(params: &Params) -> [[f64; 2]; 4] {
    CORNER_PIN_CORNERS.map(|(corner, default)| {
        let read = |axis: char, fallback: f64| {
            let value = params.number(&corner_pin_key(corner, axis), fallback);
            let value = if value.is_finite() { value } else { fallback };
            value.clamp(CORNER_PIN_MIN, CORNER_PIN_MAX) / 100.0
        };
        [read('X', default[0]), read('Y', default[1])]
    })
}

/// The pin to draw, or `None` when it is off, at the default corners, or a
/// shape that cannot be drawn in perspective (crossed, dented or flat).
pub fn read_corner_pin(params: &Params) -> Option<[[f32; 2]; 4]> {
    if !params.bool(CORNER_PIN_ENABLED, false) {
        return None;
    }
    let corners = corner_pin_corners(params);
    let identity = corners
        .iter()
        .zip(CORNER_PIN_CORNERS)
        .all(|(point, (_, default))| (point[0] - default[0] / 100.0).abs() < 1e-9 && (point[1] - default[1] / 100.0).abs() < 1e-9);
    if identity || !is_convex(&corners) {
        return None;
    }
    Some(corners.map(|[x, y]| [x as f32, y as f32]))
}

/// True when the corners make a convex shape in either winding.
pub fn is_convex(corners: &[[f64; 2]; 4]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let [ax, ay] = corners[i];
        let [bx, by] = corners[(i + 1) % 4];
        let [cx, cy] = corners[(i + 2) % 4];
        let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
        if !cross.is_finite() || cross.abs() < 1e-9 {
            return false;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// A clip's box on the canvas: centre, size and rotation in degrees. A
/// negative size (a flip) only mirrors the picture inside the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinBox {
    pub center: [f64; 2],
    pub size: [f64; 2],
    pub rotation_degrees: f64,
}

/// A box point (box units) on the canvas.
pub fn box_to_canvas(pin_box: &PinBox, [x, y]: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = pin_box.rotation_degrees.to_radians().sin_cos();
    let local = [(x - 0.5) * pin_box.size[0].abs(), (y - 0.5) * pin_box.size[1].abs()];
    [
        pin_box.center[0] + local[0] * cos - local[1] * sin,
        pin_box.center[1] + local[0] * sin + local[1] * cos,
    ]
}

/// A canvas point in box units (the inverse of [`box_to_canvas`]).
pub fn canvas_to_box(pin_box: &PinBox, [x, y]: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = pin_box.rotation_degrees.to_radians().sin_cos();
    let (dx, dy) = (x - pin_box.center[0], y - pin_box.center[1]);
    let local = [dx * cos + dy * sin, -dx * sin + dy * cos];
    [local[0] / pin_box.size[0].abs().max(1e-9) + 0.5, local[1] / pin_box.size[1].abs().max(1e-9) + 0.5]
}

/// The params that pin the corners (box units) and switch the pin on.
pub fn corner_pin_params(corners: &[[f64; 2]; 4]) -> Vec<(String, f64)> {
    CORNER_PIN_CORNERS
        .iter()
        .zip(corners)
        .flat_map(|((corner, _), [x, y])| {
            [
                (corner_pin_key(corner, 'X'), (x * 100.0).clamp(CORNER_PIN_MIN, CORNER_PIN_MAX)),
                (corner_pin_key(corner, 'Y'), (y * 100.0).clamp(CORNER_PIN_MIN, CORNER_PIN_MAX)),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/corner-pin/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("cases file")).expect("cases json")
    }

    #[test]
    fn reads_the_params_like_the_reference() {
        for case in cases()["read"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let mut params = Params::new();
            for (key, value) in case["params"].as_object().unwrap() {
                match value {
                    Value::Bool(flag) => {
                        params.set(key.clone(), *flag);
                    }
                    Value::Number(number) => {
                        params.set(key.clone(), number.as_f64().unwrap());
                    }
                    Value::String(text) => {
                        params.set(key.clone(), text.as_str());
                    }
                    _ => {}
                }
            }
            let got = read_corner_pin(&params);
            match case["corners"].as_array() {
                None => assert!(got.is_none(), "{name}: no pin, got {got:?}"),
                Some(want) => {
                    let got = got.unwrap_or_else(|| panic!("{name}: a pin"));
                    for (point, want) in got.iter().zip(want) {
                        let want = [want[0].as_f64().unwrap(), want[1].as_f64().unwrap()];
                        assert!((point[0] as f64 - want[0]).abs() < 1e-6 && (point[1] as f64 - want[1]).abs() < 1e-6, "{name}");
                    }
                }
            }
        }
    }

    #[test]
    fn places_the_corners_on_the_canvas_and_back() {
        for case in cases()["canvas"].as_array().unwrap() {
            let bounds = &case["bounds"];
            let number = |key: &str| bounds[key].as_f64().unwrap();
            let pin_box = PinBox {
                center: [number("cx"), number("cy")],
                size: [number("width"), number("height")],
                rotation_degrees: number("rotation"),
            };
            for (corner, want) in case["corners"].as_array().unwrap().iter().zip(case["canvas"].as_array().unwrap()) {
                let corner = [corner[0].as_f64().unwrap(), corner[1].as_f64().unwrap()];
                let got = box_to_canvas(&pin_box, corner);
                assert!((got[0] - want[0].as_f64().unwrap()).abs() < 1e-6 && (got[1] - want[1].as_f64().unwrap()).abs() < 1e-6);
                let back = canvas_to_box(&pin_box, got);
                assert!((back[0] - corner[0]).abs() < 1e-9 && (back[1] - corner[1]).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn writes_params_that_read_back() {
        let corners = [[0.1, 0.05], [0.85, 0.15], [0.9, 0.8], [0.05, 0.95]];
        let mut params = Params::new();
        params.set(CORNER_PIN_ENABLED, true);
        for (key, value) in corner_pin_params(&corners) {
            params.set(key, value);
        }
        let read = read_corner_pin(&params).unwrap();
        for (a, b) in read.iter().zip(corners) {
            assert!((a[0] as f64 - b[0]).abs() < 1e-6 && (a[1] as f64 - b[1]).abs() < 1e-6);
        }
        assert_eq!(corner_pin_keys().len(), 8);
    }
}

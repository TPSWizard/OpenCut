//! Corner pin: a layer's four corners pinned to points of its own box, so
//! it is drawn in perspective (a phone screen, a sign, a picture frame).
//!
//! Points are in the layer's box before its rotation: (0, 0) is its top
//! left and (1, 1) its bottom right; the corners go top left, top right,
//! bottom right, bottom left. The layer shader maps every box point back to
//! the texture through [`pin_matrix`]. Checked against
//! `fixtures/corner-pin/cases.json` (an independent reference).

/// Top left, top right, bottom right, bottom left, in box units.
pub type CornerPin = [[f32; 2]; 4];

/// A 3×3 matrix, rows first.
pub type Matrix3 = [[f64; 3]; 3];

/// True when the four corners make a convex shape, in either winding (a
/// mirrored pin is allowed). Crossed corners (a bow tie), a dent or corners
/// on one line cannot be drawn in perspective.
pub fn is_drawable_pin(corners: &[[f64; 2]; 4]) -> bool {
    if corners.iter().flatten().any(|value| !value.is_finite()) {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..4 {
        let [ax, ay] = corners[i];
        let [bx, by] = corners[(i + 1) % 4];
        let [cx, cy] = corners[(i + 2) % 4];
        let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
        if cross.abs() < 1e-9 {
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

/// The map from the unit square (texture uv) to the pinned corners: a box
/// point is `(m·[u, v, 1]).xy / (m·[u, v, 1]).z` (Heckbert's square to
/// quadrilateral). `None` when the corners cannot be drawn.
pub fn square_to_pin(corners: &[[f64; 2]; 4]) -> Option<Matrix3> {
    if !is_drawable_pin(corners) {
        return None;
    }
    let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = *corners;
    let sx = x0 - x1 + x2 - x3;
    let sy = y0 - y1 + y2 - y3;
    let (g, h) = if sx.abs() < 1e-12 && sy.abs() < 1e-12 {
        (0.0, 0.0)
    } else {
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() < 1e-12 {
            return None;
        }
        ((sx * dy2 - dx2 * sy) / den, (dx1 * sy - sx * dy1) / den)
    };
    Some([
        [x1 - x0 + g * x1, x3 - x0 + h * x3, x0],
        [y1 - y0 + g * y1, y3 - y0 + h * y3, y0],
        [g, h, 1.0],
    ])
}

/// The map from a box point back to the texture's uv: the inverse of
/// [`square_to_pin`], scaled so its third row is positive inside the pinned
/// shape (a point where it is not lies beyond the horizon and shows nothing).
pub fn pin_matrix(corners: &[[f64; 2]; 4]) -> Option<Matrix3> {
    let [[a, b, c], [d, e, f], [g, h, i]] = square_to_pin(corners)?;
    let mut inverse = [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ];
    let centre = [
        corners.iter().map(|point| point[0]).sum::<f64>() / 4.0,
        corners.iter().map(|point| point[1]).sum::<f64>() / 4.0,
    ];
    let w = inverse[2][0] * centre[0] + inverse[2][1] * centre[1] + inverse[2][2];
    if w == 0.0 || !w.is_finite() {
        return None;
    }
    // Normalise so the shader sees values near 1 whatever the scale.
    let scale = 1.0 / w;
    for row in &mut inverse {
        for value in row {
            *value *= scale;
        }
    }
    Some(inverse)
}

/// `m` applied to a point, with the perspective divide; `None` when the
/// third coordinate is not positive.
pub fn apply(m: &Matrix3, [x, y]: [f64; 2]) -> Option<[f64; 2]> {
    let w = m[2][0] * x + m[2][1] * y + m[2][2];
    if w <= 0.0 {
        return None;
    }
    Some([(m[0][0] * x + m[0][1] * y + m[0][2]) / w, (m[1][0] * x + m[1][1] * y + m[1][2]) / w])
}

/// The corners as the shader takes them, widened to f64.
pub fn widen(pin: &CornerPin) -> [[f64; 2]; 4] {
    pin.map(|[x, y]| [x as f64, y as f64])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/corner-pin/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("cases file")).expect("cases json")
    }

    fn point(value: &Value) -> [f64; 2] {
        [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
    }

    #[test]
    fn matches_the_reference_maps() {
        for case in cases()["homography"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let corners: Vec<[f64; 2]> = case["corners"].as_array().unwrap().iter().map(point).collect();
            let corners: [[f64; 2]; 4] = corners.try_into().unwrap();
            let forward = square_to_pin(&corners).unwrap_or_else(|| panic!("{name}: drawable"));
            for sample in case["forward"].as_array().unwrap() {
                let got = apply(&forward, point(&sample["uv"])).unwrap();
                let want = point(&sample["point"]);
                assert!((got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9, "{name}: {got:?} vs {want:?}");
            }
            let inverse = pin_matrix(&corners).unwrap();
            for sample in case["inverse"].as_array().unwrap() {
                let got = apply(&inverse, point(&sample["point"]));
                match sample["uv"].as_array() {
                    None => assert!(got.is_none(), "{name}: beyond the horizon, got {got:?}"),
                    Some(_) => {
                        let got = got.unwrap_or_else(|| panic!("{name}: in front"));
                        let want = point(&sample["uv"]);
                        assert!((got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9, "{name}: {got:?} vs {want:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn refuses_shapes_it_cannot_draw() {
        assert!(pin_matrix(&[[1.0, 0.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).is_none(), "bow tie");
        assert!(pin_matrix(&[[0.0, 0.0], [1.0, 0.0], [0.3, 0.3], [0.0, 1.0]]).is_none(), "dent");
        assert!(pin_matrix(&[[0.0, 0.5], [1.0, 0.5], [1.0, 0.5], [0.0, 0.5]]).is_none(), "flat");
        assert!(pin_matrix(&[[f64::NAN, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).is_none(), "not a number");
    }
}

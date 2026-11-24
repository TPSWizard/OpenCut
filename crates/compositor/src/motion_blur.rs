//! Motion blur: a layer that moves during the shutter is drawn at evenly
//! spaced points between where it is when the shutter opens and where it is
//! when it closes, and the draws are averaged (in one pass, in the layer
//! shader). Both editors pass the two quads (`LayerDescriptor::motion_blur`);
//! how many draws that takes is decided here: one per pixel of the farthest
//! corner's travel, plus one, at most [`MOTION_BLUR_MAX_SAMPLES`]. Checked
//! against `fixtures/motion-blur/cases.json`.

use crate::QuadTransformDescriptor;

/// The most draws one blurred layer takes.
pub const MOTION_BLUR_MAX_SAMPLES: u32 = 32;
/// Pixels of travel per draw.
const STEP: f64 = 1.0;
/// Less travel than this is drawn once, without blur.
const MIN_TRAVEL: f64 = 0.5;

/// The quad's corners in frame pixels: top left, top right, bottom right,
/// bottom left (before rotation).
pub fn quad_corners(quad: &QuadTransformDescriptor) -> [[f64; 2]; 4] {
    let angle = (quad.rotation_degrees as f64).to_radians();
    let (sin, cos) = angle.sin_cos();
    let half_width = quad.width as f64 / 2.0;
    let half_height = quad.height as f64 / 2.0;
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
        let x = sx * half_width;
        let y = sy * half_height;
        [quad.center_x as f64 + x * cos - y * sin, quad.center_y as f64 + x * sin + y * cos]
    })
}

/// How many draws a layer moving from `from` to `to` during the shutter
/// takes; 1 means it barely moves and is drawn once.
pub fn motion_blur_samples(from: &QuadTransformDescriptor, to: &QuadTransformDescriptor) -> u32 {
    let travel = quad_corners(from)
        .iter()
        .zip(quad_corners(to).iter())
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .fold(0.0_f64, f64::max);
    if !travel.is_finite() || travel < MIN_TRAVEL {
        return 1;
    }
    ((travel / STEP).ceil() as u32 + 1).min(MOTION_BLUR_MAX_SAMPLES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/motion-blur/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn quad(value: &Value) -> QuadTransformDescriptor {
        serde_json::from_value(value.clone()).expect("quad")
    }

    #[test]
    fn sample_counts_match_the_reference() {
        let cases = cases();
        assert_eq!(cases["maxSamples"].as_u64(), Some(MOTION_BLUR_MAX_SAMPLES as u64));
        for case in cases["samples"].as_array().expect("samples") {
            let count = motion_blur_samples(&quad(&case["from"]), &quad(&case["to"]));
            assert_eq!(count as u64, case["count"].as_u64().expect("count"), "{case}");
        }
        for case in cases["render"].as_array().expect("render") {
            let count = motion_blur_samples(&quad(&case["from"]), &quad(&case["to"]));
            assert_eq!(count as u64, case["count"].as_u64().expect("count"));
        }
    }
}

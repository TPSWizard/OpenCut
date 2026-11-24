//! Motion blur on a clip: the `motionBlur` param, 0 (off) to 100, is how
//! much of a frame the shutter stays open (100 is the whole frame, 50 the
//! film look). The renderer asks where the clip is half a shutter before
//! and after the frame's time and the compositor draws the clip along the
//! way (`compositor::motion_blur_samples`). The web editor reads the same
//! param the same way (`apps/web/src/motion-blur`); both are checked against
//! `fixtures/motion-blur/cases.json`.

use time::FrameRate;

use crate::Params;

pub const MOTION_BLUR_PARAM: &str = "motionBlur";
pub const MOTION_BLUR_MAX: f64 = 100.0;
/// Amounts below this are off.
const MOTION_BLUR_OFF: f64 = 0.5;

/// The clip's motion blur amount, 0 to 100.
pub fn read_motion_blur(params: &Params) -> f64 {
    let amount = params.number(MOTION_BLUR_PARAM, 0.0);
    if amount.is_finite() { amount.clamp(0.0, MOTION_BLUR_MAX) } else { 0.0 }
}

/// Half the time the shutter is open, in seconds, for an amount at a frame
/// rate; `None` when the blur is off.
pub fn motion_blur_half_shutter(amount: f64, fps: FrameRate) -> Option<f64> {
    let amount = if amount.is_finite() { amount.clamp(0.0, MOTION_BLUR_MAX) } else { 0.0 };
    if amount < MOTION_BLUR_OFF || fps.numerator == 0 || fps.denominator == 0 {
        return None;
    }
    Some(amount / MOTION_BLUR_MAX * fps.denominator as f64 / fps.numerator as f64 / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn half_shutters_match_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/motion-blur/cases.json");
        let cases: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
        for case in cases["shutter"].as_array().expect("shutter") {
            let fps = FrameRate {
                numerator: case["fpsNumerator"].as_u64().unwrap() as u32,
                denominator: case["fpsDenominator"].as_u64().unwrap() as u32,
            };
            let got = motion_blur_half_shutter(case["amount"].as_f64().unwrap(), fps);
            match case["halfSeconds"].as_f64() {
                Some(expected) => assert!((got.expect("on") - expected).abs() < 1e-12, "{case}"),
                None => assert_eq!(got, None, "{case}"),
            }
        }
    }
}

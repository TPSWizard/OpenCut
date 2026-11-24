//! Deflicker on a video clip: the `deflicker.strength` param (off, light,
//! medium, strong) and the clip's measured frame levels in
//! `deflicker.levels` (`deflicker::Levels`, tagged with the media they were
//! measured from, so a replaced file is not corrected with old levels).
//! The planner turns them into a brightness gain for the frame's source
//! time, which the shared compositor applies before effects. The web editor
//! reads the same params (`apps/web/src/deflicker`).

pub use deflicker::Strength as DeflickerStrength;
use deflicker::{Levels, Strength};

use crate::Params;

pub const DEFLICKER_STRENGTH: &str = "deflicker.strength";
pub const DEFLICKER_LEVELS: &str = "deflicker.levels";

/// The clip's deflicker strength (off when unset or unknown).
pub fn read_deflicker_strength(params: &Params) -> Strength {
    Strength::parse(&params.text(DEFLICKER_STRENGTH, "off")).unwrap_or(Strength::Off)
}

/// The clip's measured levels, when they were measured from `media_id`.
pub fn read_deflicker_levels(params: &Params, media_id: &str) -> Option<Levels> {
    Levels::parse(&params.text(DEFLICKER_LEVELS, "")).filter(|levels| levels.media_id == media_id)
}

/// The brightness gain for the frame at `source_seconds`, or `None` when
/// deflicker is off, unmeasured, or leaves the frame as it is.
pub fn deflicker_gain(params: &Params, media_id: &str, source_seconds: f64) -> Option<f64> {
    let window = read_deflicker_strength(params).window();
    if window < 2 {
        return None;
    }
    let gain = read_deflicker_levels(params, media_id)?.gain_at(source_seconds, window);
    ((gain - 1.0).abs() > 1e-6).then_some(gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gains_need_a_strength_and_levels_from_this_media() {
        let levels = Levels { media_id: "m".into(), start: 0.0, rate: 10.0, values: vec![100.0, 120.0, 100.0, 100.0, 100.0] };
        let mut params = Params::new();
        params.set(DEFLICKER_LEVELS, levels.encode());
        assert_eq!(deflicker_gain(&params, "m", 0.1), None, "off by default");
        params.set(DEFLICKER_STRENGTH, "light");
        let gain = deflicker_gain(&params, "m", 0.1).expect("the bright frame is pulled down");
        assert!((gain - 105.0 / 120.0).abs() < 1e-9, "{gain}");
        assert_eq!(deflicker_gain(&params, "other", 0.1), None, "levels of another file are ignored");
        params.set(DEFLICKER_STRENGTH, "bogus");
        assert_eq!(read_deflicker_strength(&params), Strength::Off);
    }
}

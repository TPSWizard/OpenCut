//! Credit rolls and crawls (row 98): a text clip that scrolls through the
//! frame, up (end credits) or to the left (a news crawl). Two element
//! params drive it (read by `editor::read_credit_roll`): `roll` (`"up"` or `"left"`; anything else is still)
//! and `rollSpeed` (canvas pixels per second; 0 or absent fits the scroll
//! to the clip, so the text enters at the clip's start and has just left
//! at its end).
//!
//! The roll sets the clip's position along its axis (`positionY` for up,
//! `positionX` for left) while it plays; the other axis, scale and
//! animations stay the clip's own. It is worked out from the text's drawn
//! box, which each renderer measures, so the same function serves the
//! desktop renderer and, through WASM, the web one.

/// Slowest and fastest scroll a person can set, in canvas pixels per second.
pub const ROLL_SPEED_MIN: f64 = 10.0;
pub const ROLL_SPEED_MAX: f64 = 4000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollDirection {
    /// From below the frame to above it (end credits).
    Up,
    /// From the right edge to past the left one (a crawl).
    Left,
}

impl RollDirection {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "up" => Some(Self::Up),
            "left" => Some(Self::Left),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Left => "left",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreditRoll {
    pub direction: RollDirection,
    /// Canvas pixels per second; `None` fits the scroll to the clip.
    pub speed: Option<f64>,
}

/// The text's drawn box relative to its anchor (the clip's position), in
/// canvas pixels after its scale: `left`/`top` are usually minus half the
/// size (centred text).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollBox {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl RollBox {
    /// A box of this size centred on the anchor.
    pub fn centred(width: f64, height: f64) -> Self {
        Self { left: -width / 2.0, top: -height / 2.0, width, height }
    }
}

/// The roll for a `roll` param and a `rollSpeed` (0, absent or not a
/// number fits the clip; other speeds are held to the allowed range).
pub fn credit_roll_from(direction: &str, speed: f64) -> Option<CreditRoll> {
    let direction = RollDirection::parse(direction)?;
    let speed = (speed.is_finite() && speed > 0.0).then(|| speed.clamp(ROLL_SPEED_MIN, ROLL_SPEED_MAX));
    Some(CreditRoll { direction, speed })
}

/// Where the anchor starts (the box just past the far edge) and ends (the
/// box just past the near edge) along the roll's axis, in canvas pixels
/// from the canvas centre.
pub fn credit_roll_span(direction: RollDirection, canvas_width: f64, canvas_height: f64, block: RollBox) -> (f64, f64) {
    match direction {
        RollDirection::Up => (canvas_height / 2.0 - block.top, -canvas_height / 2.0 - (block.top + block.height)),
        RollDirection::Left => (canvas_width / 2.0 - block.left, -canvas_width / 2.0 - (block.left + block.width)),
    }
}

/// The clip's position along the roll's axis `local_seconds` into a clip
/// `clip_seconds` long: fitted, it moves evenly from the start to the end
/// of [`credit_roll_span`] over the clip; at a speed, it moves that many
/// canvas pixels a second from the start and stops past the edge.
pub fn credit_roll_position(
    roll: CreditRoll,
    local_seconds: f64,
    clip_seconds: f64,
    canvas_width: f64,
    canvas_height: f64,
    block: RollBox,
) -> f64 {
    let (start, end) = credit_roll_span(roll.direction, canvas_width, canvas_height, block);
    let t = if local_seconds.is_finite() { local_seconds.max(0.0) } else { 0.0 };
    match roll.speed {
        Some(speed) => (start - speed * t).max(end),
        None if clip_seconds > 0.0 => start + (end - start) * (t / clip_seconds).min(1.0),
        None => start,
    }
}

/// How long a clip must be for the roll at its speed to carry the text all
/// the way through; `None` when it is fitted to the clip.
pub fn credit_roll_seconds(roll: CreditRoll, canvas_width: f64, canvas_height: f64, block: RollBox) -> Option<f64> {
    let speed = roll.speed?;
    let (start, end) = credit_roll_span(roll.direction, canvas_width, canvas_height, block);
    Some((start - end) / speed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Fixture {
        rolls: Vec<RollCase>,
    }

    #[derive(Deserialize)]
    struct RollCase {
        name: String,
        direction: String,
        speed: f64,
        local: f64,
        clip: f64,
        canvas: [f64; 2],
        rect: [f64; 4],
        expected: f64,
        seconds: Option<f64>,
    }

    #[test]
    fn rolls_match_the_reference() {
        let text = include_str!("../../../fixtures/text-templates/cases.json");
        let fixture: Fixture = serde_json::from_str(text).unwrap();
        assert!(fixture.rolls.len() >= 8);
        for case in fixture.rolls {
            let roll = credit_roll_from(&case.direction, case.speed).unwrap();
            let block = RollBox { left: case.rect[0], top: case.rect[1], width: case.rect[2], height: case.rect[3] };
            let got = credit_roll_position(roll, case.local, case.clip, case.canvas[0], case.canvas[1], block);
            assert!((got - case.expected).abs() < 1e-9, "{}: {got} vs {}", case.name, case.expected);
            let seconds = credit_roll_seconds(roll, case.canvas[0], case.canvas[1], block);
            match (seconds, case.seconds) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-9, "{}: {a} vs {b}", case.name),
                (None, None) => {}
                other => panic!("{}: {other:?}", case.name),
            }
        }
    }

    #[test]
    fn only_up_and_left_roll() {
        assert_eq!(credit_roll_from("", 0.0), None);
        assert_eq!(credit_roll_from("down", 0.0), None);
        assert_eq!(credit_roll_from("up", 2.0).unwrap().speed, Some(ROLL_SPEED_MIN));
        assert_eq!(credit_roll_from("left", f64::NAN).unwrap().speed, None);
    }
}

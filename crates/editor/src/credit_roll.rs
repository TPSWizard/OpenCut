//! Credit rolls and crawls (row 98) on text clips: the `roll` and
//! `rollSpeed` params read into the shared `titles` crate's roll, which
//! the renderers place from the text's drawn size.

use crate::Params;
pub use titles::{
    CreditRoll, ROLL_SPEED_MAX, ROLL_SPEED_MIN, RollBox, RollDirection, credit_roll_position, credit_roll_seconds,
    credit_roll_span,
};

/// The clip's roll, if its params ask for one.
pub fn read_credit_roll(params: &Params) -> Option<CreditRoll> {
    titles::credit_roll_from(&params.text("roll", ""), params.number("rollSpeed", 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_params() {
        let mut params = Params::new();
        assert_eq!(read_credit_roll(&params), None);
        params.set("roll", "up").set("rollSpeed", 120.0);
        assert_eq!(read_credit_roll(&params), Some(CreditRoll { direction: RollDirection::Up, speed: Some(120.0) }));
        params.set("rollSpeed", "fast");
        assert_eq!(read_credit_roll(&params).unwrap().speed, None);
    }
}

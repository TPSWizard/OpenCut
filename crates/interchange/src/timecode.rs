//! SMPTE timecode `HH:MM:SS:FF` at a rate's whole-frame base (30 for
//! 29.97), in drop frame (`HH:MM:SS;FF`, two frame numbers skipped each
//! minute except every tenth; four at 59.94) when the rate is 29.97 or 59.94.

use crate::Rate;

fn drop_count(rate: Rate) -> i64 {
    if rate.drop_frame() { rate.timecode_base() as i64 / 15 } else { 0 }
}

/// The timecode of frame `frames` (negative counts as 0).
pub fn frames_to_timecode(frames: i64, rate: Rate) -> String {
    let base = rate.timecode_base() as i64;
    let drop = drop_count(rate);
    let mut frames = frames.max(0);
    if drop > 0 {
        let per_ten = base * 600 - drop * 9;
        let per_minute = base * 60 - drop;
        let (tens, rest) = (frames / per_ten, frames % per_ten);
        frames += drop * 9 * tens;
        if rest > drop {
            frames += drop * ((rest - drop) / per_minute);
        }
    }
    let f = frames % base;
    let total_seconds = frames / base;
    let (h, m, s) = (total_seconds / 3600, (total_seconds / 60) % 60, total_seconds % 60);
    let separator = if drop > 0 { ';' } else { ':' };
    format!("{:02}:{:02}:{:02}{separator}{:02}", h % 100, m, s, f)
}

/// The frame of a timecode (`:` or `;` before the frames; `.` also read).
pub fn timecode_to_frames(text: &str, rate: Rate) -> Option<i64> {
    let parts: Vec<&str> = text.trim().split([':', ';', '.']).collect();
    let [h, m, s, f] = parts[..] else { return None };
    let number = |part: &str| part.parse::<i64>().ok().filter(|value| *value >= 0);
    let (h, m, s, f) = (number(h)?, number(m)?, number(s)?, number(f)?);
    let base = rate.timecode_base() as i64;
    let drop = drop_count(rate);
    let minutes = h * 60 + m;
    Some(((h * 3600 + m * 60 + s) * base + f) - drop * (minutes - minutes / 10))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_frame_skips_two_numbers_a_minute() {
        let ntsc = Rate::new(30000, 1001);
        assert_eq!(frames_to_timecode(1799, ntsc), "00:00:59;29");
        assert_eq!(frames_to_timecode(1800, ntsc), "00:01:00;02");
        assert_eq!(frames_to_timecode(17982, ntsc), "00:10:00;00");
        assert_eq!(frames_to_timecode(107892, ntsc), "01:00:00;00");
        for frames in [0, 1799, 1800, 17981, 17982, 107892, 123457] {
            assert_eq!(timecode_to_frames(&frames_to_timecode(frames, ntsc), ntsc), Some(frames));
        }
        let pal = Rate::new(25, 1);
        assert_eq!(frames_to_timecode(90000, pal), "01:00:00:00");
        assert_eq!(timecode_to_frames("01:00:00:00", pal), Some(90000));
        assert_eq!(timecode_to_frames("bad", pal), None);
    }
}

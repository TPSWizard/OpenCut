use time::{FormatTimecodeOptions, FrameRate, MediaTime, TimeCodeFormat, format_timecode};

/// `hh:mm:ss:ff` for the project's frame rate.
pub(crate) fn timecode(time: MediaTime, rate: FrameRate) -> String {
    format_timecode(FormatTimecodeOptions {
        time: time.max(MediaTime::ZERO),
        format: Some(TimeCodeFormat::HhMmSsFf),
        rate: Some(rate),
    })
    .unwrap_or_else(|| format!("{:.2}s", time.to_seconds_f64()))
}

/// `m:ss` or `m:ss.t`, for durations in lists and rulers.
pub(crate) fn short_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let whole = seconds.floor() as u64;
    let minutes = whole / 60;
    let secs = whole % 60;
    let tenths = ((seconds - whole as f64) * 10.0).round() as u64;
    if tenths == 0 || seconds >= 60.0 {
        format!("{minutes}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}.{tenths}")
    }
}

/// `YYYY-MM-DD HH:MM` in UTC from milliseconds since the epoch.
pub(crate) fn date_time(millis: u64) -> String {
    let secs = millis / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

// Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates_and_durations() {
        assert_eq!(date_time(0), "1970-01-01 00:00");
        assert_eq!(date_time(1_700_000_000_000), "2023-11-14 22:13");
        assert_eq!(short_duration(0.0), "0:00");
        assert_eq!(short_duration(5.5), "0:05.5");
        assert_eq!(short_duration(125.0), "2:05");
        assert_eq!(
            timecode(MediaTime::from_seconds_f64(61.5).unwrap(), FrameRate::FPS_30),
            "00:01:01:15"
        );
    }
}

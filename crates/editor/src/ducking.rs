//! Auto-ducking: music drops while someone speaks. The voice is measured
//! as an RMS level per short window, windows above a threshold become
//! speech intervals (short gaps are bridged), and each interval becomes a
//! dip in the music's volume keys with a ramp down before it and a ramp
//! back up after it. The web app keeps the same arithmetic in
//! `apps/web/src/ducking/envelope.ts`.

/// The level reported for a window with no signal, in dBFS.
pub const SILENCE_DB: f64 = -100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuckingOptions {
    /// How far the music drops while speech plays, in dB (negative).
    pub amount_db: f64,
    /// RMS level, in dBFS, above which a window counts as speech.
    pub threshold_db: f64,
    /// Seconds the music takes to drop before speech starts.
    pub attack_seconds: f64,
    /// Seconds the music takes to come back after speech ends.
    pub release_seconds: f64,
    /// Gaps in speech shorter than this stay ducked.
    pub hold_seconds: f64,
    /// Length of one measurement window in seconds.
    pub window_seconds: f64,
    /// Speech shorter than this is ignored.
    pub min_speech_seconds: f64,
}

impl Default for DuckingOptions {
    fn default() -> Self {
        Self {
            amount_db: -12.0,
            threshold_db: -40.0,
            attack_seconds: 0.1,
            release_seconds: 0.5,
            hold_seconds: 0.3,
            window_seconds: 0.05,
            min_speech_seconds: 0.1,
        }
    }
}

/// A stretch of timeline time, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: f64,
    pub end: f64,
}

/// Times are kept to the microsecond so window arithmetic stays exact.
fn round(seconds: f64) -> f64 {
    (seconds * 1e6).round() / 1e6
}

pub fn mean_square_to_db(mean_square: f64) -> f64 {
    if mean_square > 0.0 { (10.0 * mean_square.log10()).max(SILENCE_DB) } else { SILENCE_DB }
}

/// RMS level in dBFS of each consecutive window of mono samples.
pub fn rms_db_windows(samples: &[f32], sample_rate: u32, window_seconds: f64) -> Vec<f64> {
    let window = ((sample_rate as f64 * window_seconds).round() as usize).max(1);
    samples
        .chunks(window)
        .map(|chunk| {
            let energy: f64 = chunk.iter().map(|value| (*value as f64) * (*value as f64)).sum();
            mean_square_to_db(energy / chunk.len().max(1) as f64)
        })
        .collect()
}

/// Where speech plays, from per-window levels. Window `i` covers
/// `offset_seconds + i * window_seconds` onward. Gaps shorter than
/// `hold_seconds` are bridged; intervals shorter than `min_speech_seconds`
/// are dropped.
pub fn speech_intervals(levels: &[f64], options: &DuckingOptions, offset_seconds: f64) -> Vec<Interval> {
    let mut raw: Vec<Interval> = Vec::new();
    let mut open: Option<Interval> = None;
    for (index, level) in levels.iter().enumerate() {
        let start = round(offset_seconds + index as f64 * options.window_seconds);
        let end = round(start + options.window_seconds);
        if *level >= options.threshold_db {
            match open.as_mut() {
                Some(interval) => interval.end = end,
                None => open = Some(Interval { start, end }),
            }
        } else if let Some(interval) = open.take() {
            raw.push(interval);
        }
    }
    if let Some(interval) = open {
        raw.push(interval);
    }
    let mut merged: Vec<Interval> = Vec::new();
    for interval in raw {
        match merged.last_mut() {
            Some(last) if interval.start - last.end < options.hold_seconds => last.end = interval.end,
            _ => merged.push(interval),
        }
    }
    merged.retain(|interval| interval.end - interval.start >= options.min_speech_seconds - 1e-9);
    merged
}

/// A volume key: seconds from the start of the music clip, and dB.
pub type DuckingKey = (f64, f64);

/// Volume keys that dip the music for each speech interval (timeline
/// seconds). Ramps that would cross each other merge into one dip; keys
/// outside the clip collapse onto its edges, the latest winning at the
/// start and the earliest at the end so a dip still running at the end
/// stays down.
pub fn build_ducking_keys(
    intervals: &[Interval],
    clip_start: f64,
    clip_duration: f64,
    base_db: f64,
    options: &DuckingOptions,
) -> Vec<DuckingKey> {
    let ducked = base_db + options.amount_db;
    let clip_end = clip_start + clip_duration;
    let mut in_clip: Vec<Interval> = intervals
        .iter()
        .copied()
        .filter(|interval| interval.end > clip_start && interval.start < clip_end)
        .collect();
    in_clip.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    if in_clip.is_empty() || clip_duration <= 0.0 {
        return Vec::new();
    }
    let mut dips: Vec<Interval> = Vec::new();
    for interval in in_clip {
        match dips.last_mut() {
            Some(last) if interval.start - options.attack_seconds <= last.end + options.release_seconds => {
                last.end = last.end.max(interval.end)
            }
            _ => dips.push(interval),
        }
    }
    let mut keys: Vec<DuckingKey> = Vec::new();
    let mut push = |time: f64, value: f64| {
        let clamped = round((time - clip_start).clamp(0.0, clip_duration));
        if let Some(last) = keys.last_mut() {
            if (last.0 - clamped).abs() < 1e-6 {
                if time - clip_start < clip_duration {
                    last.1 = value;
                }
                return;
            }
        }
        keys.push((clamped, value));
    };
    for dip in dips {
        push(dip.start - options.attack_seconds, base_db);
        push(dip.start, ducked);
        push(dip.end, ducked);
        push(dip.end + options.release_seconds, base_db);
    }
    keys
}

/// Seconds of speech inside a set of intervals.
pub fn total_seconds(intervals: &[Interval]) -> f64 {
    intervals.iter().map(|interval| interval.end - interval.start).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_are_measured_in_dbfs() {
        let mut samples = vec![0.0f32; 300];
        samples[100..200].fill(0.5);
        let levels = rms_db_windows(&samples, 100, 1.0);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], SILENCE_DB);
        assert!((levels[1] - 20.0 * 0.5f64.log10()).abs() < 1e-6);
        assert_eq!(levels[2], SILENCE_DB);
    }

    #[test]
    fn loud_windows_become_intervals_and_short_gaps_are_bridged() {
        let (loud, quiet) = (-20.0, -60.0);
        let levels = [quiet, loud, loud, quiet, loud, quiet, quiet, quiet, loud, loud];
        let options = DuckingOptions { window_seconds: 0.1, hold_seconds: 0.15, ..Default::default() };
        let intervals = speech_intervals(&levels, &options, 10.0);
        assert_eq!(
            intervals,
            [Interval { start: 10.1, end: 10.5 }, Interval { start: 10.8, end: 11.0 }]
        );
    }

    #[test]
    fn speech_shorter_than_the_minimum_is_dropped() {
        let (loud, quiet) = (-20.0, -60.0);
        let options = DuckingOptions { hold_seconds: 0.01, ..Default::default() };
        let intervals = speech_intervals(&[quiet, loud, quiet, loud, loud, loud], &options, 0.0);
        assert_eq!(intervals, [Interval { start: 0.15, end: 0.3 }]);
    }

    #[test]
    fn keys_ramp_down_before_speech_and_back_up_after() {
        let keys = build_ducking_keys(
            &[Interval { start: 5.0, end: 7.0 }],
            4.0,
            10.0,
            -3.0,
            &DuckingOptions::default(),
        );
        assert_eq!(keys, [(0.9, -3.0), (1.0, -15.0), (3.0, -15.0), (3.5, -3.0)]);
    }

    #[test]
    fn crossing_ramps_merge_and_edges_clamp() {
        let options = DuckingOptions { amount_db: -10.0, ..Default::default() };
        let keys = build_ducking_keys(
            &[
                Interval { start: 0.0, end: 1.0 },
                Interval { start: 1.3, end: 2.0 },
                Interval { start: 9.8, end: 12.0 },
            ],
            0.0,
            10.0,
            0.0,
            &options,
        );
        assert_eq!(
            keys,
            [(0.0, -10.0), (2.0, -10.0), (2.5, 0.0), (9.7, 0.0), (9.8, -10.0), (10.0, -10.0)]
        );
    }

    #[test]
    fn no_overlap_writes_nothing() {
        let keys = build_ducking_keys(
            &[Interval { start: 20.0, end: 25.0 }],
            0.0,
            10.0,
            0.0,
            &DuckingOptions::default(),
        );
        assert!(keys.is_empty());
    }
}

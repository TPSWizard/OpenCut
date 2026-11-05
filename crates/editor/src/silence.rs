//! Silence auto-cut: stretches of a clip whose level stays under a
//! threshold for long enough are removed, keeping a little padding of
//! silence on each side of the speech so cuts do not clip words. The web
//! app keeps the same arithmetic in `apps/web/src/silence/detect.ts`.

use crate::ducking::Interval;

/// What happens to the rest of the timeline when a stretch is removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RippleScope {
    /// Everything that starts after the stretch moves left, on every track.
    #[default]
    All,
    /// Only the element's own track closes up.
    Track,
    /// The gap stays.
    None,
}

impl RippleScope {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "track" => Some(Self::Track),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Track => "track",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SilenceOptions {
    /// RMS level, in dBFS, under which a window counts as silent.
    pub threshold_db: f64,
    /// Silence shorter than this stays.
    pub min_silence_seconds: f64,
    /// Silence kept on each side of the speech around a cut.
    pub padding_seconds: f64,
    /// Length of one measurement window in seconds.
    pub window_seconds: f64,
}

impl Default for SilenceOptions {
    fn default() -> Self {
        Self { threshold_db: -40.0, min_silence_seconds: 0.5, padding_seconds: 0.1, window_seconds: 0.05 }
    }
}

/// Times are kept to the microsecond so window arithmetic stays exact.
fn round(seconds: f64) -> f64 {
    (seconds * 1e6).round() / 1e6
}

/// The stretches to remove from a clip, in timeline seconds, from the
/// per-window levels of its own audio. Window `i` covers
/// `clip_start + i * window_seconds` onward. Silence at the clip's edges is
/// removed up to the edge; silence between speech keeps `padding_seconds`
/// on each side.
pub fn silence_intervals(levels: &[f64], clip_start: f64, clip_end: f64, options: &SilenceOptions) -> Vec<Interval> {
    let mut runs: Vec<Interval> = Vec::new();
    let mut open: Option<Interval> = None;
    for (index, level) in levels.iter().enumerate() {
        let start = round(clip_start + index as f64 * options.window_seconds);
        let end = round(clip_end.min(start + options.window_seconds));
        if end <= start {
            break;
        }
        if *level < options.threshold_db {
            match open.as_mut() {
                Some(run) => run.end = end,
                None => open = Some(Interval { start, end }),
            }
        } else if let Some(run) = open.take() {
            runs.push(run);
        }
    }
    if let Some(run) = open {
        runs.push(run);
    }
    let edge = 1e-6;
    runs.into_iter()
        .filter(|run| run.end - run.start >= options.min_silence_seconds - edge)
        .filter_map(|run| {
            let touches_start = run.start <= clip_start + edge;
            let touches_end = run.end >= clip_end - edge;
            let start = if touches_start { clip_start } else { round(run.start + options.padding_seconds) };
            let end = if touches_end { clip_end } else { round(run.end - options.padding_seconds) };
            (end - start >= options.window_seconds - edge).then_some(Interval { start, end })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: f64 = -20.0;
    const QUIET: f64 = -70.0;

    fn levels(parts: &[(usize, f64)]) -> Vec<f64> {
        parts.iter().flat_map(|(count, level)| std::iter::repeat_n(*level, *count)).collect()
    }

    fn tenth() -> SilenceOptions {
        SilenceOptions { window_seconds: 0.1, ..Default::default() }
    }

    #[test]
    fn long_quiet_stretches_go_with_padding_next_to_speech() {
        let cuts = silence_intervals(&levels(&[(5, LOUD), (15, QUIET), (10, LOUD)]), 10.0, 13.0, &tenth());
        assert_eq!(cuts, [Interval { start: 10.6, end: 11.9 }]);
    }

    #[test]
    fn silence_at_the_edges_goes_up_to_the_edge() {
        let cuts = silence_intervals(&levels(&[(10, QUIET), (10, LOUD), (10, QUIET)]), 0.0, 3.0, &tenth());
        assert_eq!(cuts, [Interval { start: 0.0, end: 0.9 }, Interval { start: 2.1, end: 3.0 }]);
    }

    #[test]
    fn short_pauses_stay() {
        let cuts = silence_intervals(&levels(&[(5, LOUD), (4, QUIET), (5, LOUD)]), 0.0, 1.4, &tenth());
        assert!(cuts.is_empty());
    }

    #[test]
    fn a_cut_the_padding_swallows_is_dropped() {
        let options = SilenceOptions { padding_seconds: 0.3, ..tenth() };
        let cuts = silence_intervals(&levels(&[(5, LOUD), (6, QUIET), (5, LOUD)]), 0.0, 1.6, &options);
        assert!(cuts.is_empty());
    }
}

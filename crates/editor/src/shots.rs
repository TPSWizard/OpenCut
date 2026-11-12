//! Shot (scene-change) cuts from per-frame scores, and where they land on
//! the timeline. The desktop scores frames with FFmpeg's `scdet`; the web
//! app with colour histograms (`apps/web/src/shots/detect.ts`); the cut
//! picking is the same on both.

use time::MediaTime;

use crate::document::Element;

/// A frame's difference from the one before it, 0 (same) to 100.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameScore {
    /// Source time in seconds.
    pub seconds: f64,
    pub score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotOptions {
    /// A score at or above this is a cut.
    pub threshold: f64,
    /// Cuts closer than this to the previous cut are dropped (the stronger stays).
    pub min_gap_seconds: f64,
}

impl Default for ShotOptions {
    fn default() -> Self {
        Self { threshold: 20.0, min_gap_seconds: 0.5 }
    }
}

/// The cuts among frame scores: at or above the threshold, at least the
/// minimum gap apart; of a cluster the strongest survives.
pub fn pick_cuts(scores: &[FrameScore], options: &ShotOptions) -> Vec<FrameScore> {
    let mut cuts: Vec<FrameScore> = Vec::new();
    for frame in scores {
        if frame.score < options.threshold {
            continue;
        }
        if let Some(last) = cuts.last_mut() {
            if frame.seconds - last.seconds < options.min_gap_seconds {
                if frame.score > last.score {
                    *last = *frame;
                }
                continue;
            }
        }
        cuts.push(*frame);
    }
    cuts
}

/// Where a source time of the clip lands on the timeline, or none when it
/// is trimmed away or sits at the clip's edges. Speed changes are not
/// followed.
pub fn source_to_timeline(element: &Element, source_seconds: f64) -> Option<MediaTime> {
    let source = MediaTime::from_seconds_f64(source_seconds)?;
    if source <= element.trim_start || source >= element.trim_start + element.duration {
        return None;
    }
    Some(element.start_time + (source - element.trim_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(seconds: f64, score: f64) -> FrameScore {
        FrameScore { seconds, score }
    }

    #[test]
    fn strong_changes_far_enough_apart_are_cuts() {
        let scores = [score(0.1, 2.0), score(2.0, 60.0), score(2.1, 80.0), score(2.3, 40.0), score(4.0, 35.0), score(5.0, 10.0)];
        let options = ShotOptions { threshold: 30.0, min_gap_seconds: 0.5 };
        assert_eq!(pick_cuts(&scores, &options), vec![score(2.1, 80.0), score(4.0, 35.0)]);
        assert!(pick_cuts(&scores, &ShotOptions { threshold: 90.0, ..options }).is_empty());
    }

    #[test]
    fn cuts_map_through_the_trim() {
        let mut element = Element::text("x", MediaTime::from_seconds_f64(10.0).unwrap());
        element.duration = MediaTime::from_seconds_f64(4.0).unwrap();
        element.trim_start = MediaTime::from_seconds_f64(1.0).unwrap();
        assert_eq!(source_to_timeline(&element, 2.0), MediaTime::from_seconds_f64(11.0));
        assert_eq!(source_to_timeline(&element, 1.0), None);
        assert_eq!(source_to_timeline(&element, 5.0), None);
    }
}

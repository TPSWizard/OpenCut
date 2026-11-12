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

/// A normalised RGB histogram (8 bins per channel) of tightly packed RGB
/// pixels: the web app's `histogram` in `src/shots/detect.ts`.
pub fn colour_histogram(rgb: &[u8]) -> Vec<f32> {
    let mut counts = vec![0f32; 512];
    let pixels = rgb.len() / 3;
    if pixels == 0 {
        return counts;
    }
    for p in rgb.chunks_exact(3) {
        let (r, g, b) = ((p[0] >> 5) as usize, (p[1] >> 5) as usize, (p[2] >> 5) as usize);
        counts[(r * 8 + g) * 8 + b] += 1.0;
    }
    for bin in &mut counts {
        *bin /= pixels as f32;
    }
    counts
}

/// Half the L1 distance between two normalised histograms, 0 to 100 (the
/// web app's shot score; 30 and up is a cut there).
pub fn histogram_distance(a: &[f32], b: &[f32]) -> f64 {
    let sum: f64 = a.iter().zip(b).map(|(x, y)| (x - y).abs() as f64).sum();
    (sum / 2.0 * 100.0).min(100.0)
}

/// The web app's cut threshold on [`histogram_distance`].
pub const HISTOGRAM_CUT_THRESHOLD: f64 = 30.0;

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
    fn colour_histograms_tell_shots_apart() {
        let blue = [96u8, 110, 128].repeat(100);
        let green = [60u8, 120, 70].repeat(100);
        let a = colour_histogram(&blue);
        assert!((a.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert_eq!(histogram_distance(&a, &a), 0.0);
        assert_eq!(histogram_distance(&a, &colour_histogram(&green)), 100.0);
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

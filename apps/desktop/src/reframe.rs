//! Auto-reframe on the desktop: the clip's source span is decoded small
//! through FFmpeg, the subject in each sample is guessed from movement and
//! skin tone (there is no face detector here; the web editor adds
//! MediaPipe faces), shot changes from colour histograms (as on the web),
//! and
//! `editor::plan_reframe` turns that into the scale and position keys.

use std::path::Path;

use editor::{ReframeSample, Retime, ShotOptions};
use media::{Ffmpeg, VideoReader};

use crate::i18n::t;

/// Frames a second sampled for the subject.
pub(crate) const SAMPLE_FPS: f64 = 5.0;
/// The decoded width; the height follows the source's shape.
const SAMPLE_WIDTH: u32 = editor::SUBJECT_FRAME_WIDTH as u32;

pub(crate) struct ReframeAnalysis {
    pub samples: Vec<ReframeSample>,
    /// Shot changes in clip seconds.
    pub cuts: Vec<f64>,
}

pub(crate) struct ReframeSource<'a> {
    pub path: &'a Path,
    pub size: (u32, u32),
    /// Source seconds where the clip starts.
    pub trim_start: f64,
    /// Clip length on the timeline, seconds.
    pub clip_duration: f64,
    pub retime: Option<&'a Retime>,
}

impl ReframeSource<'_> {
    /// Source seconds the clip plays (its length after any speed change).
    fn source_span(&self) -> f64 {
        match self.retime {
            Some(retime) => retime.forward(self.clip_duration),
            None => self.clip_duration,
        }
    }

    fn clip_seconds(&self, source_seconds: f64) -> f64 {
        editor::clip_seconds_for_source(self.retime, (source_seconds - self.trim_start).max(0.0), self.clip_duration)
    }
}

pub(crate) fn analyse(ffmpeg: &Ffmpeg, source: &ReframeSource, reset_at_shots: bool) -> Result<ReframeAnalysis, String> {
    let from = source.trim_start;
    let to = from + source.source_span();
    let mut reader = VideoReader::open(ffmpeg, source.path, from, SAMPLE_WIDTH, source.size, SAMPLE_FPS)
        .map_err(|error| error.to_string())?;
    let (width, height) = (reader.width() as usize, reader.height() as usize);
    let mut frames: Vec<(f64, Vec<u8>)> = Vec::new();
    let mut scores = Vec::new();
    let mut previous: Option<Vec<f32>> = None;
    while let Some(frame) = reader.next_frame().map_err(|error| error.to_string())? {
        if frame.time_seconds >= to {
            break;
        }
        let rgb: Vec<u8> = frame.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        let histogram = editor::colour_histogram(&rgb);
        let time = source.clip_seconds(frame.time_seconds);
        if let Some(last) = &previous {
            scores.push(editor::FrameScore { seconds: time, score: editor::histogram_distance(last, &histogram) });
        }
        previous = Some(histogram);
        frames.push((time, rgb));
    }
    if frames.is_empty() {
        return Err(t("No frames could be read from the clip.").to_owned());
    }
    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
    scores.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    // Shot changes by colour, as the web editor finds them; the background
    // the subject stands out from is worked out per shot either way.
    let all_cuts: Vec<f64> =
        editor::pick_cuts(&scores, &ShotOptions { threshold: editor::HISTOGRAM_CUT_THRESHOLD, min_gap_seconds: 0.5 })
            .into_iter()
            .map(|cut| cut.seconds)
            .filter(|seconds| *seconds > 0.0 && *seconds < source.clip_duration)
            .collect();
    let mut samples = Vec::with_capacity(frames.len());
    let bounds: Vec<f64> = std::iter::once(f64::NEG_INFINITY).chain(all_cuts.iter().copied()).chain(std::iter::once(f64::INFINITY)).collect();
    for pair in bounds.windows(2) {
        let shot: Vec<&(f64, Vec<u8>)> = frames.iter().filter(|(time, _)| *time >= pair[0] && *time < pair[1]).collect();
        let pictures: Vec<&[u8]> = shot.iter().map(|(_, rgb)| rgb.as_slice()).collect();
        for ((time, _), subject) in shot.iter().zip(editor::shot_subjects(&pictures, width, height)) {
            samples.push(ReframeSample { time: *time, subject });
        }
    }
    let cuts = if reset_at_shots { all_cuts } else { Vec::new() };
    Ok(ReframeAnalysis { samples, cuts })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OPENCUT_REFRAME_CHECK=<mp4>` runs the analysis on the synthetic
    /// fixture `.cache/reframe-face.mp4` (640x360, a drawn face walking from
    /// x 0.22 to 0.52 over 3 s, then a cut to a second shot with it at 0.78).
    #[test]
    fn follows_the_fixture_subject() {
        let Ok(path) = std::env::var("OPENCUT_REFRAME_CHECK") else { return };
        let ffmpeg = Ffmpeg::locate().expect("ffmpeg");
        let source = ReframeSource {
            path: Path::new(&path),
            size: (640, 360),
            trim_start: 0.0,
            clip_duration: 6.0,
            retime: None,
        };
        let analysis = analyse(&ffmpeg, &source, true).unwrap();
        assert_eq!(analysis.cuts.len(), 1, "{:?}", analysis.cuts);
        assert!((analysis.cuts[0] - 3.0).abs() < 0.25);
        let geometry = editor::ReframeGeometry::new(640.0, 360.0, 1080.0, 1920.0);
        let plan = editor::plan_reframe(&analysis.samples, &analysis.cuts, &geometry, editor::ReframeCamera::Smooth);
        let centre = |px: f64| 0.5 - px / geometry.shown_width;
        let keys: Vec<(f64, f64)> = plan.keys_x.iter().map(|(t, v)| (*t, centre(*v))).collect();
        println!("REFRAME keys {keys:?}");
        assert_eq!(plan.shots, 2);
        let at = |time: f64| {
            let after = keys.iter().position(|k| k.0 >= time).unwrap_or(keys.len() - 1);
            if after == 0 { return keys[0].1 }
            let (a, b) = (keys[after - 1], keys[after]);
            a.1 + (b.1 - a.1) * ((time - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
        };
        // The face's centre: 0.22 + 0.1 t in the first shot, 0.78 in the second.
        for (time, face) in [(0.5, 0.27), (1.5, 0.37), (2.5, 0.47), (4.0, 0.78), (5.5, 0.78)] {
            assert!((at(time) - face).abs() < 0.08, "at {time}: crop {} vs face {face}", at(time));
        }
    }
}

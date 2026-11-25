//! Motion tracking on the desktop (row 71): the video is decoded small
//! through FFmpeg from the playhead and the point is followed with the
//! shared `tracking` crate (the web editor runs the same code as WASM).

use std::path::PathBuf;

use editor::Retime;
use media::{Ffmpeg, VideoReader};
use tracking::{Gray, TrackSample, Tracker};

use crate::i18n::t;

/// Frames are tracked at this width.
pub(crate) const TRACK_WIDTH: u32 = 320;
/// At most this many frames a second are looked at.
pub(crate) const TRACK_FPS: f64 = 30.0;
/// The tracked box, as a share of the frame's shorter side.
pub(crate) const TRACK_BOX: f64 = 0.1;

/// What to track.
pub(crate) struct TrackJob {
    pub path: PathBuf,
    pub size: (u32, u32),
    /// Source seconds where the clip starts.
    pub trim_start: f64,
    pub clip_duration: f64,
    pub retime: Option<Retime>,
    /// Timeline seconds where the video clip starts.
    pub video_start: f64,
    /// Timeline seconds to track from and to.
    pub from: f64,
    pub to: f64,
    /// The start point as shares of the video's frame.
    pub start: (f64, f64),
    /// More points tracked in the same pass (corner pin tracks four).
    pub more: Vec<(f64, f64)>,
}

/// The tracked samples (timeline seconds, shares of the frame) and how many
/// frames lost the point.
pub(crate) fn track(ffmpeg: &Ffmpeg, job: &TrackJob) -> Result<(Vec<TrackSample>, usize), String> {
    let (mut tracks, lost) = track_points(ffmpeg, job)?;
    Ok((tracks.swap_remove(0), lost))
}

/// The start point and `more` points tracked in one pass: one run of
/// samples per point (the same times), and how many frames lost any.
pub(crate) fn track_points(ffmpeg: &Ffmpeg, job: &TrackJob) -> Result<(Vec<Vec<TrackSample>>, usize), String> {
    let source_at = |timeline: f64| {
        let offset = match &job.retime {
            Some(retime) if !retime.is_identity() => retime.source_offset(timeline - job.video_start, job.clip_duration),
            _ => timeline - job.video_start,
        };
        job.trim_start + offset.max(0.0)
    };
    let (source_from, source_to) = {
        let (a, b) = (source_at(job.from), source_at(job.to));
        (a.min(b), a.max(b))
    };
    let mut reader =
        VideoReader::open(ffmpeg, &job.path, source_from, TRACK_WIDTH, job.size, TRACK_FPS).map_err(|error| error.to_string())?;
    let (width, height) = (reader.width() as usize, reader.height() as usize);
    let box_size = (TRACK_BOX * width.min(height) as f64).max(16.0);
    let starts: Vec<(f64, f64)> = std::iter::once(job.start).chain(job.more.iter().copied()).collect();
    let mut trackers: Vec<Tracker> = Vec::new();
    let mut samples: Vec<Vec<TrackSample>> = vec![Vec::new(); starts.len()];
    let mut lost = 0;
    while let Some(frame) = reader.next_frame().map_err(|error| error.to_string())? {
        if frame.time_seconds > source_to + 1e-6 {
            break;
        }
        let gray = Gray::from_bgra(&frame.bgra, width, height);
        let positions: Vec<(f64, f64)> = if trackers.is_empty() {
            for start in &starts {
                trackers.push(Tracker::new(&gray, start.0 * width as f64, start.1 * height as f64, box_size, box_size)?);
            }
            trackers.iter().map(Tracker::position).collect()
        } else {
            let steps: Vec<_> = trackers.iter_mut().map(|tracker| tracker.step(&gray)).collect();
            if steps.iter().any(|step| step.lost) {
                lost += 1;
            }
            steps.iter().map(|step| (step.x, step.y)).collect()
        };
        let clip = editor::clip_seconds_for_source(job.retime.as_ref(), (frame.time_seconds - job.trim_start).max(0.0), job.clip_duration);
        for (run, (x, y)) in samples.iter_mut().zip(positions) {
            run.push(TrackSample { time: job.video_start + clip, x: x / width as f64, y: y / height as f64 });
        }
    }
    if samples[0].len() < 2 {
        return Err(t("Not enough of the video to track from there.").to_owned());
    }
    Ok((samples, lost))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white box moving right at 60 px a second over grey noise, tracked
    /// through FFmpeg-decoded frames. Needs FFmpeg; skips otherwise.
    #[test]
    fn tracks_a_moving_box_in_a_real_video() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("box.mp4");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=320x180:r=30:d=2,noise=alls=30:allf=u"])
            .args(["-f", "lavfi", "-i", "color=c=white:s=24x24:r=30:d=2"])
            // drawbox does not move with `t`; overlay does.
            .args(["-filter_complex", "[0][1]overlay=x='40+t*60':y=60:shortest=1", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let job = TrackJob {
            path,
            size: (320, 180),
            trim_start: 0.0,
            clip_duration: 2.0,
            retime: None,
            video_start: 10.0,
            from: 10.0,
            to: 11.5,
            start: (52.0 / 320.0, 72.0 / 180.0),
            more: Vec::new(),
        };
        let (samples, lost) = track(&ffmpeg, &job).unwrap();
        assert_eq!(lost, 0);
        assert!(samples.len() >= 40, "{} samples", samples.len());
        for sample in &samples {
            let expected = 52.0 + (sample.time - 10.0) * 60.0;
            assert!((sample.x * 320.0 - expected).abs() <= 2.5, "{sample:?} vs x {expected}");
            assert!((sample.y * 180.0 - 72.0).abs() <= 2.5, "{sample:?}");
        }
        assert!((samples[0].time - 10.0).abs() < 1e-6 && samples.last().unwrap().time <= 11.5 + 1e-6);
    }

    /// Several points in one pass (the tracked corner pin's four corners):
    /// two corners of the moving box both slide right with it.
    #[test]
    fn tracks_several_points_in_one_pass() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("box.mp4");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=320x180:r=30:d=2,noise=alls=30:allf=u"])
            .args(["-f", "lavfi", "-i", "color=c=white:s=48x48:r=30:d=2,noise=alls=40:allf=u"])
            .args(["-filter_complex", "[0][1]overlay=x='40+t*60':y=60:shortest=1", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let job = TrackJob {
            path,
            size: (320, 180),
            trim_start: 0.0,
            clip_duration: 2.0,
            retime: None,
            video_start: 0.0,
            from: 0.0,
            to: 1.5,
            start: (44.0 / 320.0, 64.0 / 180.0),
            more: vec![(84.0 / 320.0, 104.0 / 180.0)],
        };
        let (tracks, _) = track_points(&ffmpeg, &job).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].len(), tracks[1].len());
        let last = tracks[0].len() - 1;
        for track in &tracks {
            let moved = (track[last].x - track[0].x) * 320.0;
            let expected = (track[last].time - track[0].time) * 60.0;
            assert!((moved - expected).abs() < 4.0, "moved {moved} px, expected {expected}");
        }
    }
}

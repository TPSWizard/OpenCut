//! Smooth jump cuts (row 106) on the desktop: for each planned bridge the
//! frame before it and the frame after it are decoded from the clip's file
//! at full size, the shared `jumpcut` crate makes the in-between frames,
//! and they are encoded (H.264, no sound) into ~/.opencut/processed. The
//! clips are then put on the timeline in one step by
//! [`editor::Command::InsertJumpCutBridges`].

use std::path::Path;

use editor::{Command, JumpCutBridgeJob, MediaItem, Project};
use media::{EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg, VideoReader};

/// One frame of `source` at `seconds`, full size (BGRA).
fn frame_at(ffmpeg: &Ffmpeg, source: &MediaItem, seconds: f64) -> Result<(Vec<u8>, u32, u32), String> {
    let size = (source.width.unwrap_or(1920).max(2), source.height.unwrap_or(1080).max(2));
    let fps = source.fps.filter(|fps| *fps > 0.0).unwrap_or(30.0);
    let mut reader = VideoReader::open(ffmpeg, &source.path, seconds.max(0.0), size.0, size, fps).map_err(|error| error.to_string())?;
    let frame = reader
        .next_frame()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("No frame at {seconds:.3} s in {}", source.name))?;
    Ok((frame.bgra, reader.width(), reader.height()))
}

/// Writes the bridge for `job` from `source` (the file both clips show) to
/// `output`: `job.plan.frames` frames at `fps`. Returns the median motion
/// found, in pixels of the picture.
pub(crate) fn make_bridge_file(ffmpeg: &Ffmpeg, source: &MediaItem, job: &JumpCutBridgeJob, fps: f64, output: &Path) -> Result<f64, String> {
    let (p, width, height) = frame_at(ffmpeg, source, job.from_source)?;
    let (q, q_width, q_height) = frame_at(ffmpeg, source, job.to_source)?;
    if (width, height) != (q_width, q_height) {
        return Err("The frames on either side of the cut differ in size".to_owned());
    }
    let (w, h) = (width as usize, height as usize);
    let field = jumpcut::motion_between(&p, &q, w, h).ok_or("The frames could not be compared")?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let settings = EncodeSettings {
        output: output.to_path_buf(),
        width,
        height,
        fps,
        format: EncodeFormat::Mp4,
        quality: EncodeQuality::VeryHigh,
        platform: None,
        codec: None,
        video_bitrate: None,
        hardware: false,
        duration_seconds: job.plan.duration,
        audio: Vec::new(),
        disclosure: None,
        alpha: None,
    };
    let mut encoder = Encoder::start(ffmpeg, &settings).map_err(|error| error.to_string())?;
    for alpha in &job.plan.alphas {
        let frame = jumpcut::in_between(&p, &q, w, h, &field, *alpha);
        encoder.write_frame(&frame).map_err(|error| error.to_string())?;
    }
    encoder.finish().map_err(|error| error.to_string())?;
    Ok(field.median())
}

/// The commands that add each made bridge file and put the bridges on the
/// timeline: one undo step when run as a batch.
pub(crate) fn bridge_commands(project: &Project, made: &[(JumpCutBridgeJob, MediaItem)]) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut bridges = Vec::new();
    for (job, item) in made {
        let Some(element) = editor::jump_cut_bridge_element(project, job, &item.id) else { continue };
        commands.push(Command::AddMedia(item.clone()));
        commands.push(Command::SetOrigin {
            media_id: item.id.clone(),
            origin: Some(editor::MediaOrigin::generated("smooth-cut", Some(&job.media_id), None)),
        });
        bridges.push((job.cut.track_id.clone(), element));
    }
    if !bridges.is_empty() {
        commands.push(Command::InsertJumpCutBridges { bridges });
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Element, MediaKind, TrackPlacement, apply};
    use time::MediaTime;

    fn decode_all(ffmpeg: &Ffmpeg, path: &Path, size: (u32, u32), fps: f64) -> Vec<Vec<u8>> {
        let mut reader = VideoReader::open(ffmpeg, path, 0.0, size.0, size, fps).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = reader.next_frame().unwrap() {
            frames.push(frame.bgra);
        }
        frames
    }

    fn mae(a: &[u8], b: &[u8], w: usize, h: usize, margin: usize) -> f64 {
        let mut total = 0.0;
        let mut n = 0.0;
        for y in margin..h - margin {
            for x in margin..w - margin {
                for c in 0..3 {
                    let i = (y * w + x) * 4 + c;
                    total += (f64::from(a[i]) - f64::from(b[i])).abs();
                    n += 1.0;
                }
            }
        }
        total / n
    }

    /// A camera panning steadily across a still picture (2 px a frame) is
    /// cut at 1 s, jumping 0.5 s ahead in the file. The bridge made through
    /// FFmpeg and the shared crate lands above the cut, and its frames are
    /// close to the frames the camera really passed through (a steady pan
    /// has its true in-betweens in the file), far closer than a crossfade.
    /// The real renderer then draws the bridge over the cut. Needs FFmpeg;
    /// the render part needs a GPU adapter.
    #[test]
    fn bridges_a_jump_cut_with_frames_close_to_the_real_ones() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let still = dir.path().join("still.png");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=800x300:r=1:d=1", "-frames:v", "1"])
            .arg(&still)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let take = dir.path().join("take.mp4");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-loop", "1", "-framerate", "30", "-t", "3", "-i"])
            .arg(&still)
            .args(["-vf", "crop=320:240:x='20+2*n':y=30", "-c:v", "libx264", "-crf", "12", "-pix_fmt", "yuv420p"])
            .arg(&take)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));

        let mut project = Project::new("Smooth cuts");
        project.settings.canvas = editor::CanvasSize { width: 320, height: 240 };
        let item = MediaItem {
            id: "take".into(),
            name: "take.mp4".into(),
            kind: MediaKind::Video,
            path: take.clone(),
            duration: MediaTime::from_seconds_f64(3.0),
            width: Some(320),
            height: Some(240),
            fps: Some(30.0),
            has_audio: false,
        };
        apply(&mut project, &Command::AddMedia(item.clone())).unwrap();
        let at = |seconds: f64| MediaTime::from_seconds_f64(seconds).unwrap();
        let mut first = Element::from_media(&item, MediaTime::ZERO);
        first.duration = at(1.0);
        first.trim_end = at(2.0);
        let mut second = Element::from_media(&item, at(1.0));
        second.trim_start = at(1.5);
        second.duration = at(1.0);
        second.trim_end = at(0.5);
        for element in [first, second] {
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let cuts = editor::project_jump_cuts(&project);
        assert_eq!(cuts.len(), 1);
        let job = editor::plan_jump_cut_bridge(&project, &cuts[0], 6).unwrap();
        assert_eq!(job.plan.frames, 6);

        let output = dir.path().join("bridge.mp4");
        let moved = make_bridge_file(&ffmpeg, &item, &job, 30.0, &output).unwrap();
        // From source 0.867 s to 1.6 s: 22 frames of 2 px.
        assert!((moved - 44.0).abs() < 4.0, "motion {moved} px");
        let bridge = decode_all(&ffmpeg, &output, (320, 240), 30.0);
        assert_eq!(bridge.len(), 6);
        let source = decode_all(&ffmpeg, &take, (320, 240), 30.0);
        let frame_of = |seconds: f64| &source[(seconds * 30.0).round() as usize];
        let (p, q) = (frame_of(job.from_source), frame_of(job.to_source));
        for (index, alpha) in job.plan.alphas.iter().enumerate() {
            // The camera's real frame part way through the jump.
            let truth = frame_of(job.from_source + alpha * (job.to_source - job.from_source));
            let fade: Vec<u8> = p.iter().zip(q).map(|(a, b)| ((1.0 - alpha) * f64::from(*a) + alpha * f64::from(*b)).round() as u8).collect();
            let (ours, crossfade) = (mae(&bridge[index], truth, 320, 240, 24), mae(&fade, truth, 320, 240, 24));
            eprintln!("frame {index} α {alpha:.3}: {ours:.2} vs crossfade {crossfade:.2}");
            assert!(ours < crossfade * 0.5, "frame {index}: {ours} vs crossfade {crossfade}");
        }

        let bridge_item = MediaItem {
            id: "bridge".into(),
            name: "bridge.mp4".into(),
            path: output.clone(),
            duration: MediaTime::from_seconds_f64(0.2),
            ..item.clone()
        };
        let commands = bridge_commands(&project, &[(job.clone(), bridge_item)]);
        apply(&mut project, &Command::Batch(commands)).unwrap();
        assert!(editor::project_jump_cuts(&project)[0].smoothed);
        let main = project.main_track_index();
        assert_eq!(project.tracks[main - 1].elements.len(), 1, "the bridge sits on the track above");

        use render::{Renderer, RendererOptions};
        let Ok(mut renderer) = Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), ..Default::default() }) else {
            eprintln!("no GPU adapter; skipping the render");
            return;
        };
        // At the cut the program shows the bridge's fourth frame, not the second clip's first.
        let frame = renderer.render(&project, at(1.0), 320, 240).unwrap();
        let shown = mae(&frame.bgra, &bridge[3], 320, 240, 24);
        let unsmoothed = mae(&frame.bgra, q, 320, 240, 24);
        eprintln!("render at the cut: {shown:.2} from the bridge frame, {unsmoothed:.2} from the frame after");
        assert!(shown < 3.0 && shown < unsmoothed * 0.5, "{shown} vs {unsmoothed}");
    }
}

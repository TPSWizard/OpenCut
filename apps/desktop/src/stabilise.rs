//! Stabilise on the desktop (row 107): the clip's frames are read with
//! FFmpeg (`stabilise::MEASURE_WIDTH` wide, at the file's own rate) over the
//! part of the file the clip plays, and the camera's movement between each
//! pair is measured with the shared `stabilise` crate. The camera path goes
//! on the clip (`stabilise.path`); the renderer turns it into a per-frame
//! correction. Nothing is rewritten. The web editor measures the same way
//! (`apps/web/src/stabilise`).

use std::path::PathBuf;

use media::{Ffmpeg, VideoReader};
use stabilise::{CameraPath, MEASURE_WIDTH, frame_step, measure_rate};

/// What to measure.
pub(crate) struct MeasureJob {
    pub path: PathBuf,
    pub size: (u32, u32),
    pub fps: Option<f64>,
    pub media_id: String,
    /// Source seconds the clip plays from and to.
    pub from: f64,
    pub to: f64,
}

pub(crate) fn measure(ffmpeg: &Ffmpeg, job: &MeasureJob) -> Result<CameraPath, String> {
    let rate = measure_rate(job.fps);
    let mut reader =
        VideoReader::open(ffmpeg, &job.path, job.from, MEASURE_WIDTH, job.size, rate).map_err(|error| error.to_string())?;
    let (width, height) = (reader.width() as usize, reader.height() as usize);
    let mut start = None;
    let mut previous: Option<Vec<u8>> = None;
    let mut steps = Vec::new();
    while let Some(frame) = reader.next_frame().map_err(|error| error.to_string())? {
        if frame.time_seconds > job.to + 0.5 / rate {
            break;
        }
        start.get_or_insert(frame.time_seconds);
        if let Some(previous) = &previous {
            steps.push(frame_step(previous, &frame.bgra, width, height));
        }
        previous = Some(frame.bgra);
    }
    let Some(start) = start else {
        return Err("No frames could be read from the clip.".to_owned());
    };
    let aspect = job.size.1.max(1) as f64 / job.size.0.max(1) as f64;
    Ok(CameraPath { media_id: job.media_id.clone(), start, rate, aspect, steps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Command, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
    use render::{Renderer, RendererOptions};
    use time::MediaTime;

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    /// Where FFmpeg's crop window is in frame `n` (the camera's position;
    /// whole pixels, so crop's own rounding does not come into it).
    fn window(n: usize) -> (f64, f64) {
        let n = n as f64;
        (40.0 + 2.0 * n + (14.0 * (n * 1.3).sin()).floor(), 30.0 + (10.0 * (n * 1.7).cos()).floor())
    }

    /// The test pattern panned 2 px a frame, shaken by up to 14 px across and 10 down.
    fn shaky_clip(ffmpeg: &Ffmpeg, dir: &std::path::Path) -> PathBuf {
        let still = dir.join("still.png");
        let status = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=640x320:r=1:d=1", "-frames:v", "1"])
            .arg(&still)
            .status()
            .unwrap();
        assert!(status.success());
        let path = dir.join("shaky.mp4");
        let status = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-loop", "1", "-framerate", "30", "-t", "3", "-i"])
            .arg(&still)
            .args(["-vf", "crop=320:240:x='40+2*n+floor(14*sin(n*1.3))':y='30+floor(10*cos(n*1.7))'"])
            .args(["-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    /// The shift between two BGRA frames (pixels), by the crate's own fit.
    fn moved(a: &[u8], b: &[u8], width: usize, height: usize) -> (f64, f64) {
        let step = frame_step(a, b, width, height).expect("measurable");
        (step.tx * width as f64, step.ty * width as f64)
    }

    #[test]
    fn measures_a_shaky_pan_and_the_render_holds_the_camera_steady() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let path = shaky_clip(&ffmpeg, dir.path());

        // The part of the file a clip trimmed to 0.5..2.5 s plays.
        let job = MeasureJob { path: path.clone(), size: (320, 240), fps: Some(30.0), media_id: "shaky".into(), from: 0.5, to: 2.5 };
        let started = std::time::Instant::now();
        let camera = measure(&ffmpeg, &job).unwrap();
        eprintln!("measured {} frames in {:.2} s", camera.frames(), started.elapsed().as_secs_f64());
        assert!((camera.start - 0.5).abs() < 0.02, "starts at {}", camera.start);
        assert!((60..=62).contains(&camera.frames()), "{} frames", camera.frames());
        // Each step against the true move of the crop window (the picture
        // moves the other way), in pixels of the 320-wide file.
        let first = (camera.start * 30.0).round() as usize;
        let mut worst: f64 = 0.0;
        for (k, step) in camera.steps.iter().enumerate() {
            let step = step.expect("every step measured");
            let (a, b) = (window(first + k), window(first + k + 1));
            let (tx, ty) = (step.tx * 320.0, step.ty * 320.0);
            let error = (tx + (b.0 - a.0)).abs().max((ty + (b.1 - a.1)).abs());
            if error > 0.3 {
                eprintln!("step {k}: measured ({tx:.2}, {ty:.2}) true ({:.2}, {:.2}) angle {:.4} scale {:.4}", a.0 - b.0, a.1 - b.1, step.angle(), step.scale());
            }
            worst = worst.max(error);
        }
        eprintln!("worst step error {worst:.3} px");
        assert!(worst < 0.5, "worst step error {worst:.3} px");

        let Ok(mut renderer) =
            Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), fonts_dir: None, stickers_dir: None, proxy_dir: None })
        else {
            return;
        };
        let mut project = Project::new("Stabilise");
        project.settings.canvas = editor::CanvasSize { width: 320, height: 240 };
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "shaky".into(),
                name: "shaky.mp4".into(),
                kind: MediaKind::Video,
                path,
                duration: Some(seconds(3.0)),
                width: Some(320),
                height: Some(240),
                fps: Some(30.0),
                has_audio: false,
            }),
        )
        .unwrap();
        let mut element = Element::from_media(project.media_item("shaky").unwrap(), seconds(0.0));
        element.trim_start = seconds(0.5);
        element.duration = seconds(2.0);
        element.params.set(editor::STABILISE_PATH, camera.encode());
        let element_id = element.id.clone();
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();

        let mut render = |project: &Project| -> Vec<Vec<u8>> {
            (5..35).map(|frame| renderer.render(project, seconds(frame as f64 / 30.0 + 0.002), 320, 240).unwrap().bgra).collect()
        };
        // The jitter: how far each frame-to-frame move is from the average move.
        let jitter = |frames: &[Vec<u8>]| -> f64 {
            let moves: Vec<(f64, f64)> = frames.windows(2).map(|pair| moved(&pair[0], &pair[1], 320, 240)).collect();
            let mean = moves.iter().fold((0.0, 0.0), |s, m| (s.0 + m.0, s.1 + m.1));
            let mean = (mean.0 / moves.len() as f64, mean.1 / moves.len() as f64);
            moves.iter().map(|m| (m.0 - mean.0).hypot(m.1 - mean.1)).sum::<f64>() / moves.len() as f64
        };
        let before = render(&project);
        let shaky = jitter(&before);
        for (strength, most) in [("medium", 0.25), ("still", 0.15)] {
            let mut params = editor::Params::new();
            params.set(editor::STABILISE_STRENGTH, strength);
            apply(
                &mut project,
                &Command::UpdateElement {
                    element_id: element_id.clone(),
                    patch: editor::ElementPatch { params: Some(params), ..Default::default() },
                },
            )
            .unwrap();
            let after = render(&project);
            let steady = jitter(&after);
            let zoom = editor::stabilise_corrections(&project.element(&element_id).unwrap().1.params, "shaky").unwrap().zoom;
            eprintln!("{strength}: jitter {shaky:.2} px -> {steady:.2} px, zoom {zoom:.3}");
            assert!(steady < shaky * most, "{strength}: jitter {shaky:.2} px -> {steady:.2} px");
            if strength == "still" {
                // A tripod: hardly any move between frames at all.
                let moves: Vec<f64> = after.windows(2).map(|p| { let m = moved(&p[0], &p[1], 320, 240); m.0.hypot(m.1) }).collect();
                let mean = moves.iter().sum::<f64>() / moves.len() as f64;
                eprintln!("still: mean move {mean:.2} px a frame");
                assert!(mean < 0.6, "still: mean move {mean:.2} px a frame");
            }
        }
    }
}

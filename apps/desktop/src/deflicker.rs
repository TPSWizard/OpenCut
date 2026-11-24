//! Deflicker on the desktop: the clip's frames are read with FFmpeg
//! (`deflicker::MEASURE_WIDTH` wide, `deflicker::MEASURE_FPS` a second) over
//! the part of the file the clip plays, and each one's level is measured
//! with the shared `deflicker` crate. The levels go on the clip
//! (`deflicker.levels`); the renderer turns them into a per-frame gain. The
//! web editor measures the same way (`apps/web/src/deflicker`).

use std::path::PathBuf;

use deflicker::{Levels, MEASURE_FPS, MEASURE_WIDTH, frame_level_bgra};
use media::{Ffmpeg, VideoReader};

/// What to measure.
pub(crate) struct MeasureJob {
    pub path: PathBuf,
    pub size: (u32, u32),
    pub media_id: String,
    /// Source seconds the clip plays from and to.
    pub from: f64,
    pub to: f64,
}

pub(crate) fn measure(ffmpeg: &Ffmpeg, job: &MeasureJob) -> Result<Levels, String> {
    let mut reader = VideoReader::open(ffmpeg, &job.path, job.from, MEASURE_WIDTH, job.size, MEASURE_FPS)
        .map_err(|error| error.to_string())?;
    let (width, height) = (reader.width() as usize, reader.height() as usize);
    let mut start = None;
    let mut values = Vec::new();
    while let Some(frame) = reader.next_frame().map_err(|error| error.to_string())? {
        if frame.time_seconds > job.to + 0.5 / MEASURE_FPS {
            break;
        }
        start.get_or_insert(frame.time_seconds);
        values.push(frame_level_bgra(&frame.bgra, width, height));
    }
    let Some(start) = start else {
        return Err("No frames could be read from the clip.".to_owned());
    };
    Ok(Levels { media_id: job.media_id.clone(), start, rate: MEASURE_FPS, values })
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

    /// Mean Rec. 709 luma of a rendered BGRA frame.
    fn frame_luma(frame: &render::RenderedFrame) -> f64 {
        frame_level_bgra(&frame.bgra, frame.width as usize, frame.height as usize)
    }

    fn spread(values: &[f64]) -> f64 {
        values.windows(2).map(|pair| (pair[1] - pair[0]).abs()).sum::<f64>() / (values.len() - 1) as f64
    }

    #[test]
    fn measures_a_flickering_video_and_the_render_evens_it_out() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flicker.mp4");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=4"])
            .args(["-vf", "eq=brightness='0.07*sin(n*2.3)':eval=frame", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));

        // The part of the file a clip trimmed to 1..3 s plays.
        let job = MeasureJob { path: path.clone(), size: (320, 180), media_id: "flicker".into(), from: 1.0, to: 3.0 };
        let levels = measure(&ffmpeg, &job).unwrap();
        assert!((levels.start - 1.0).abs() < 0.02, "starts at {}", levels.start);
        assert!((60..=62).contains(&levels.values.len()), "{} frames", levels.values.len());
        assert!(spread(&levels.values) > 8.0, "the file flickers: {:?}", &levels.values[..6]);

        let Ok(mut renderer) =
            Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), fonts_dir: None, stickers_dir: None, proxy_dir: None })
        else {
            return;
        };
        let mut project = Project::new("Deflicker");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "flicker".into(),
                name: "flicker.mp4".into(),
                kind: MediaKind::Video,
                path,
                duration: Some(seconds(4.0)),
                width: Some(320),
                height: Some(180),
                fps: Some(30.0),
                has_audio: false,
            }),
        )
        .unwrap();
        let mut element = Element::from_media(project.media_item("flicker").unwrap(), seconds(0.0));
        element.trim_start = seconds(1.0);
        element.duration = seconds(2.0);
        element.params.set(editor::DEFLICKER_LEVELS, levels.encode());
        let element_id = element.id.clone();
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();

        let mut lumas = |project: &Project| -> Vec<f64> {
            (10..30).map(|frame| frame_luma(&renderer.render(project, seconds(frame as f64 / 30.0), 160, 90).unwrap())).collect()
        };
        let before = lumas(&project);
        let mut params = editor::Params::new();
        params.set(editor::DEFLICKER_STRENGTH, "medium");
        apply(
            &mut project,
            &Command::UpdateElement { element_id, patch: editor::ElementPatch { params: Some(params), ..Default::default() } },
        )
        .unwrap();
        let after = lumas(&project);
        eprintln!("frame-to-frame change: before {:.2}, after {:.2}", spread(&before), spread(&after));
        assert!(spread(&before) > 8.0, "{before:?}");
        assert!(spread(&after) < spread(&before) / 3.0, "before {before:?} after {after:?}");
    }
}

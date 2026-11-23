//! Cover pictures (row 70): the cover project (`editor::cover_project`,
//! the project re-fitted to the cover's size with the title as a text clip)
//! drawn by the normal renderer, saved as a PNG, embedded in MP4 exports,
//! and a frame suggested by `editor::frame_score_bgra`.

use std::path::Path;

use editor::{CanvasSize, CoverSettings, Project, cover_project, cover_sample_times, frame_score_bgra, pick_cover_time};
use media::Ffmpeg;
use time::MediaTime;

use crate::renderer::Renderer;

/// Width the frames are scored at when suggesting one.
const SCORE_WIDTH: u32 = 160;

fn clamp_time(project: &Project, seconds: f64) -> MediaTime {
    let fps = project.settings.fps.as_f64().unwrap_or(30.0).max(1.0);
    let last = (project.duration().to_seconds_f64() - 1.0 / fps).max(0.0);
    MediaTime::from_seconds_f64(seconds.clamp(0.0, last)).unwrap_or(MediaTime::ZERO)
}

/// The cover as RGBA pixels at `size` (rounded down to even sides).
pub fn render_cover(renderer: &mut Renderer, project: &Project, cover: &CoverSettings, size: CanvasSize) -> Result<image::RgbaImage, String> {
    let drawn = cover_project(project, size, cover.time, cover.style, &cover.title);
    let frame = renderer
        .render(&drawn, clamp_time(project, cover.time), size.width, size.height)
        .map_err(|error| error.to_string())?;
    let mut rgba = frame.bgra;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    image::RgbaImage::from_raw(frame.width, frame.height, rgba).ok_or_else(|| "The cover frame has the wrong size".to_owned())
}

/// Saves the cover as a PNG.
pub fn save_cover_png(renderer: &mut Renderer, project: &Project, cover: &CoverSettings, size: CanvasSize, path: &Path) -> Result<(), String> {
    render_cover(renderer, project, cover, size)?.save(path).map_err(|error| error.to_string())
}

/// Embeds the project's cover (at the canvas size) into the MP4 at `video`
/// when the project has one set to embed. Returns whether it did.
pub fn embed_cover(renderer: &mut Renderer, ffmpeg: &Ffmpeg, project: &Project, video: &Path) -> Result<bool, String> {
    let Some(cover) = project.settings.cover.as_ref().filter(|cover| cover.embed) else {
        return Ok(false);
    };
    let png = video.with_extension("cover.png");
    save_cover_png(renderer, project, cover, project.settings.canvas, &png)?;
    let attached = media::attach_cover(ffmpeg, video, &png).map_err(|error| error.to_string());
    std::fs::remove_file(&png).ok();
    attached.map(|()| true)
}

/// The timeline seconds of the best-looking frame among evenly spaced samples.
pub fn suggest_cover_time(renderer: &mut Renderer, project: &Project) -> Result<f64, String> {
    let duration = project.duration().to_seconds_f64();
    if duration <= 0.0 {
        return Err("The timeline is empty".to_owned());
    }
    let canvas = project.settings.canvas;
    let height = ((SCORE_WIDTH as f64 * canvas.height as f64 / canvas.width.max(1) as f64).round() as u32).max(2);
    let mut samples = Vec::new();
    for time in cover_sample_times(duration, editor::COVER_SAMPLES) {
        let frame = renderer
            .render(project, clamp_time(project, time), SCORE_WIDTH, height)
            .map_err(|error| error.to_string())?;
        let score = frame_score_bgra(&frame.bgra, frame.width as usize, frame.height as usize);
        samples.push((time, score.score));
    }
    Ok(pick_cover_time(&samples, duration))
}

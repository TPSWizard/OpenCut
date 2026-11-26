//! More export kinds (row 96): an animated GIF (the shared `gifenc`
//! crate, the same bytes as the web editor), audio only (WAV, AAC or
//! Opus from the export mix), a still PNG and a PNG image sequence.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use editor::Project;
use media::{AudioFileFormat, Ffmpeg};
use time::MediaTime;

use crate::export::{MixLoudness, mastered_mix, mix_clips, temp_mix_dir};
use crate::renderer::Renderer;

/// GIF settings: the size (`width`; the height keeps the canvas shape),
/// frame rate, colours and dithering.
#[derive(Clone, Debug, PartialEq)]
pub struct GifJob {
    pub output: PathBuf,
    pub width: u32,
    pub fps: f64,
    /// Palette size, 2 to 255.
    pub colours: usize,
    pub dither: bool,
}

impl GifJob {
    /// The usual GIF: 480 px wide, 15 fps, 255 colours, dithered.
    pub fn new(output: PathBuf) -> Self {
        Self { output, width: 480, fps: 15.0, colours: gifenc::MAX_COLOURS, dither: true }
    }
}

/// Frames sampled for the palette, at most.
const PALETTE_SAMPLES: u64 = 40;

/// The GIF's frame size for a canvas: `width` (no wider than the canvas),
/// the height in proportion; both even, as the renderer draws them.
pub fn gif_size(canvas_width: u32, canvas_height: u32, width: u32) -> (u32, u32) {
    gifenc::gif_size(canvas_width, canvas_height, width)
}

fn rgba_of(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = bgra.to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    rgba
}

/// Renders the project to an animated GIF: a few frames for the palette,
/// then every frame. `progress` hears (steps done, steps total).
pub fn export_gif(
    renderer: &mut Renderer,
    project: &Project,
    job: &GifJob,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let duration = project.duration().to_seconds_f64();
    if duration <= 0.0 {
        return Err("The timeline is empty".to_owned());
    }
    let fps = job.fps.clamp(1.0, 50.0);
    let frames = ((duration * fps).ceil() as u64).max(1);
    let canvas = project.settings.canvas;
    let (width, height) = gif_size(canvas.width, canvas.height, job.width);
    let every = frames.div_ceil(PALETTE_SAMPLES);
    let samples = frames.div_ceil(every);
    let total = samples + frames;
    let time_of = |index: u64| MediaTime::from_seconds_f64(index as f64 / fps).unwrap_or(MediaTime::ZERO);

    let mut histogram = gifenc::Histogram::new();
    for (done, index) in (0..frames).step_by(every as usize).enumerate() {
        if cancelled() {
            return Err("Export cancelled".to_owned());
        }
        let frame = renderer.render(project, time_of(index), width, height).map_err(|error| error.to_string())?;
        histogram.add_rgba(&rgba_of(&frame.bgra));
        progress(done as u64 + 1, total);
    }
    let mut writer = gifenc::GifWriter::new(width, height, histogram.palette(job.colours), job.dither);
    let delays = gifenc::frame_delays(frames as usize, fps);
    for index in 0..frames {
        if cancelled() {
            return Err("Export cancelled".to_owned());
        }
        let frame = renderer.render(project, time_of(index), width, height).map_err(|error| error.to_string())?;
        writer.add_rgba(&rgba_of(&frame.bgra), delays[index as usize]);
        if index % 5 == 0 || index + 1 == frames {
            progress(samples + index + 1, total);
        }
    }
    if let Some(parent) = job.output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&job.output, writer.finish()).map_err(|error| error.to_string())?;
    Ok(job.output.clone())
}

/// Writes the project's sound alone: the export mix (with whole-mix
/// loudness when asked) as `format`. Returns the path and what loudness did.
pub fn export_audio(
    ffmpeg: &Arc<Ffmpeg>,
    project: &Project,
    output: &Path,
    format: AudioFileFormat,
    loudness: Option<MixLoudness>,
    cache_dir: Option<PathBuf>,
) -> Result<(PathBuf, Option<editor::MasterReport>), String> {
    let duration = project.duration().to_seconds_f64();
    if duration <= 0.0 {
        return Err("The timeline is empty".to_owned());
    }
    let clips = mix_clips(ffmpeg, project, cache_dir);
    if clips.is_empty() {
        return Err("The timeline has no sound".to_owned());
    }
    let dir = temp_mix_dir();
    let result = (|| {
        let (source, report) = match loudness {
            Some(loudness) => {
                let (clip, report) = mastered_mix(ffmpeg, &clips, duration, loudness, &dir)?;
                (clip.path, Some(report))
            }
            None => {
                std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
                let mix = dir.join("mix.wav");
                media::render_audio_mix(ffmpeg, &clips, duration, &mix).map_err(|error| error.to_string())?;
                (mix, None)
            }
        };
        media::encode_audio_file(ffmpeg, &source, output, format, 192).map_err(|error| error.to_string())?;
        Ok((output.to_path_buf(), report))
    })();
    std::fs::remove_dir_all(&dir).ok();
    result
}

/// Saves the frame at `time` as a PNG at the canvas size.
pub fn export_png(renderer: &mut Renderer, project: &Project, time: MediaTime, output: &Path) -> Result<PathBuf, String> {
    let canvas = project.settings.canvas;
    let frame = renderer.render(project, time, canvas.width, canvas.height).map_err(|error| error.to_string())?;
    save_png(frame.width, frame.height, &frame.bgra, output)?;
    Ok(output.to_path_buf())
}

fn save_png(width: u32, height: u32, bgra: &[u8], output: &Path) -> Result<(), String> {
    let image = image::RgbaImage::from_raw(width, height, rgba_of(bgra)).ok_or_else(|| "The frame has the wrong size".to_owned())?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    image.save_with_format(output, image::ImageFormat::Png).map_err(|error| error.to_string())
}

/// File name of frame `index` (from 1) in an image sequence.
pub fn sequence_file_name(index: u64) -> String {
    format!("frame_{index:06}.png")
}

/// Renders every frame at the project's frame rate and canvas size into
/// `folder` as `frame_000001.png`, `frame_000002.png`, ... Returns the
/// folder and how many frames were written.
pub fn export_png_sequence(
    renderer: &mut Renderer,
    project: &Project,
    folder: &Path,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(PathBuf, u64), String> {
    let duration = project.duration();
    if duration <= MediaTime::ZERO {
        return Err("The timeline is empty".to_owned());
    }
    let rate = project.settings.fps;
    let fps = rate.as_f64().unwrap_or(30.0);
    let frames = ((duration.to_seconds_f64() * fps).ceil() as u64).max(1);
    let canvas = project.settings.canvas;
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    for index in 0..frames {
        if cancelled() {
            return Err("Export cancelled".to_owned());
        }
        let time = MediaTime::from_frame(index as i64, rate).unwrap_or(MediaTime::ZERO);
        let frame = renderer.render(project, time, canvas.width, canvas.height).map_err(|error| error.to_string())?;
        save_png(frame.width, frame.height, &frame.bgra, &folder.join(sequence_file_name(index + 1)))?;
        if index % 5 == 0 || index + 1 == frames {
            progress(index + 1, frames);
        }
    }
    Ok((folder.to_path_buf(), frames))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_names() {
        assert_eq!(gif_size(1920, 1080, 480), (480, 270));
        assert_eq!(gif_size(1080, 1920, 480), (480, 852));
        assert_eq!(gif_size(320, 240, 640), (320, 240));
        assert_eq!(sequence_file_name(12), "frame_000012.png");
    }
}

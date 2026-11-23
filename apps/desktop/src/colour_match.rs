//! Reads the frames colour match measures (`editor::colour_match_commands`
//! writes the result): video frames through FFmpeg at a small width,
//! pictures decoded once and sampled on a grid.

use std::path::{Path, PathBuf};

use editor::{ColourAccumulator, ColourStats};
use media::Ffmpeg;

use crate::i18n::{t, tf};

/// Width the frames are measured at.
const SAMPLE_WIDTH: u32 = 160;
/// Frames of each target clip measured.
pub(crate) const TARGET_SAMPLES: usize = 5;

/// One clip to measure: its media and the source times to look at.
pub(crate) struct FrameSource {
    pub name: String,
    pub path: PathBuf,
    pub is_image: bool,
    pub size: (u32, u32),
    pub source_seconds: Vec<f64>,
}

/// Adds every `step`-th pixel of a BGRA frame in both directions.
fn add_grid(accumulator: &mut ColourAccumulator, bgra: &[u8], width: u32, height: u32) {
    let step = (width / SAMPLE_WIDTH).max(1) as usize;
    let (width, height) = (width as usize, height as usize);
    let mut row = Vec::with_capacity(width / step * 4 + 4);
    for y in (0..height).step_by(step) {
        row.clear();
        for x in (0..width).step_by(step) {
            let index = (y * width + x) * 4;
            if let Some(pixel) = bgra.get(index..index + 4) {
                row.extend_from_slice(pixel);
            }
        }
        accumulator.add_bgra(&row);
    }
}

/// The Lab statistics of a clip's frames.
pub(crate) fn measure(ffmpeg: Option<&Ffmpeg>, source: &FrameSource) -> Result<ColourStats, String> {
    let mut accumulator = ColourAccumulator::new();
    if source.is_image {
        let frame = media::decode_image(&source.path).map_err(|error| error.to_string())?;
        add_grid(&mut accumulator, &frame.bgra, frame.width, frame.height);
    } else {
        let ffmpeg = ffmpeg.ok_or_else(|| t("FFmpeg is needed to read video frames.").to_owned())?;
        for seconds in &source.source_seconds {
            let frame = media::thumbnail(ffmpeg, Path::new(&source.path), seconds.max(0.0), SAMPLE_WIDTH, source.size)
                .map_err(|error| error.to_string())?;
            add_grid(&mut accumulator, &frame.bgra, frame.width, frame.height);
        }
    }
    accumulator.stats().ok_or_else(|| tf("No pixels could be read from {name}.", &[("name", &source.name)]))
}

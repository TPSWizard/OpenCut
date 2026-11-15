//! Motion tools that rewrite a whole video file through FFmpeg:
//!
//! - **Stabilisation**: two passes of vid.stab (`vidstabdetect` measures
//!   the shake, `vidstabtransform` smooths the camera path and zooms just
//!   enough to hide the moving edges). vid.stab is only in GPL FFmpeg
//!   builds (gyan.dev's "full" build, most Linux packages); on others
//!   stabilisation is unavailable. FFmpeg's own `deshake` was tried as a
//!   fallback and left out: on test footage it did not reduce the shake.
//! - **In-between frames**: `minterpolate` with motion-compensated
//!   interpolation, for slow motion that does not stutter.
//!
//! Both write an H.264 MP4 the same length as the source with its audio,
//! so a clip can play the result with its trims unchanged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use crate::codecs::{VideoCodec, pick_encoder};
use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// How hard to smooth the camera path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StabiliseStrength {
    Light,
    Medium,
    Strong,
}

impl StabiliseStrength {
    pub const ALL: [Self; 3] = [Self::Light, Self::Medium, Self::Strong];

    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Medium => "medium",
            Self::Strong => "strong",
        }
    }

    /// Frames each side averaged into the smoothed path (vid.stab `smoothing`).
    pub fn smoothing(self) -> u32 {
        match self {
            Self::Light => 10,
            Self::Medium => 20,
            Self::Strong => 40,
        }
    }

    /// How shaky vid.stab assumes the footage is (1–10).
    pub fn shakiness(self) -> u32 {
        match self {
            Self::Light => 4,
            Self::Medium => 6,
            Self::Strong => 8,
        }
    }
}

type Cache<T> = OnceLock<Mutex<HashMap<PathBuf, T>>>;
static FILTERS: Cache<Vec<String>> = OnceLock::new();

/// The filters this FFmpeg build lists (`ffmpeg -filters`).
pub fn listed_filters(ffmpeg: &Ffmpeg) -> Vec<String> {
    let cache = FILTERS.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(ffmpeg.ffmpeg_path()) {
        return found.clone();
    }
    let mut command = ffmpeg.command();
    command.arg("-filters").stdout(Stdio::piped());
    let names: Vec<String> = command
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split_whitespace();
                    let flags = parts.next()?;
                    let name = parts.next()?;
                    let kinds = parts.next()?;
                    (flags.len() <= 3 && kinds.contains("->")).then(|| name.to_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    cache.lock().unwrap().insert(ffmpeg.ffmpeg_path().to_path_buf(), names.clone());
    names
}

/// Whether the build has vid.stab (both of its filters).
pub fn can_stabilise(ffmpeg: &Ffmpeg) -> bool {
    let filters = listed_filters(ffmpeg);
    let has = |name: &str| filters.iter().any(|filter| filter == name);
    has("vidstabdetect") && has("vidstabtransform")
}

/// Whether the build can make in-between frames.
pub fn can_interpolate(ffmpeg: &Ffmpeg) -> bool {
    listed_filters(ffmpeg).iter().any(|filter| filter == "minterpolate")
}

/// The two passes' filter graphs for `strength`: detection writes the
/// camera path to `transforms`, the transform pass smooths it. The file is
/// named relative to FFmpeg's working directory (filter options cannot hold
/// a Windows drive colon unescaped).
pub fn stabilise_filters(strength: StabiliseStrength, transforms: &str) -> (String, String) {
    (
        format!("vidstabdetect=shakiness={}:accuracy=15:result={transforms}", strength.shakiness()),
        format!(
            "vidstabtransform=input={transforms}:smoothing={}:optzoom=1:interpol=bicubic,unsharp=5:5:0.8:3:3:0.4",
            strength.smoothing()
        ),
    )
}

/// The `minterpolate` graph for `factor` times the source frame rate.
pub fn interpolate_filter(source_fps: f64, factor: u32) -> String {
    let fps = source_fps * f64::from(factor.clamp(2, 8));
    format!("minterpolate=fps={fps:.3}:mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1")
}

fn run(mut command: std::process::Command) -> Result<(), MediaError> {
    let result = command.output()?;
    if !result.status.success() {
        return Err(MediaError::ProgramFailed {
            program: "ffmpeg".to_owned(),
            message: crate::ffmpeg::last_stderr_line(&result.stderr),
        });
    }
    Ok(())
}

/// Encodes `source` through `filter` into `output` (an MP4, H.264 at high
/// quality, the first audio stream as AAC when there is one), writing to a
/// partial file first.
fn encode_through(ffmpeg: &Ffmpeg, source: &Path, output: &Path, filter: &str, workdir: &Path) -> Result<(), MediaError> {
    let encoder = pick_encoder(ffmpeg, VideoCodec::H264, false)?;
    let partial = output.with_extension("partial.mp4");
    let mut command = ffmpeg.command();
    command
        .current_dir(workdir)
        .arg("-y")
        .arg("-i")
        .arg(source)
        .args(["-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn"])
        .arg("-vf")
        .arg(filter)
        .args(["-c:v", &encoder]);
    if encoder == "libx264" {
        command.args(["-preset", "medium", "-crf", "18"]);
    } else {
        command.args(["-b:v", "12M"]);
    }
    command
        .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"])
        .arg(&partial);
    if let Err(error) = run(command) {
        std::fs::remove_file(&partial).ok();
        return Err(error);
    }
    std::fs::rename(&partial, output)?;
    Ok(())
}

fn prepare(output: &Path) -> Result<PathBuf, MediaError> {
    let parent = output.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&parent)?;
    Ok(parent)
}

/// Writes a stabilised copy of `source` to `output` (MP4).
pub fn stabilise_video(ffmpeg: &Ffmpeg, source: &Path, output: &Path, strength: StabiliseStrength) -> Result<(), MediaError> {
    if !can_stabilise(ffmpeg) {
        return Err(MediaError::Invalid(
            "This FFmpeg has no vid.stab, which stabilising needs (it comes with GPL builds such as gyan.dev's full build)"
                .to_owned(),
        ));
    }
    let workdir = prepare(output)?;
    let stem = output.file_stem().and_then(|stem| stem.to_str()).unwrap_or("stabilise");
    let transforms: String =
        format!("{stem}.trf").chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    let (detect, transform) = stabilise_filters(strength, &transforms);
    let result = (|| {
        let mut command = ffmpeg.command();
        command
            .current_dir(&workdir)
            .arg("-y")
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf", &detect, "-f", "null", "-"]);
        run(command)?;
        encode_through(ffmpeg, source, output, &transform, &workdir)
    })();
    std::fs::remove_file(workdir.join(&transforms)).ok();
    result
}

/// Writes `source` with `factor` times its frames (motion-interpolated) to
/// `output` (MP4); returns the new frame rate.
pub fn interpolate_video(
    ffmpeg: &Ffmpeg,
    source: &Path,
    output: &Path,
    source_fps: f64,
    factor: u32,
) -> Result<f64, MediaError> {
    if !(source_fps.is_finite() && source_fps > 0.0) {
        return Err(MediaError::Invalid("The clip's frame rate is unknown".to_owned()));
    }
    if !can_interpolate(ffmpeg) {
        return Err(MediaError::Invalid("This FFmpeg has no minterpolate filter".to_owned()));
    }
    let workdir = prepare(output)?;
    let factor = factor.clamp(2, 8);
    encode_through(ffmpeg, source, output, &interpolate_filter(source_fps, factor), &workdir)?;
    Ok(source_fps * f64::from(factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strengths_map_to_filter_settings() {
        let (detect, transform) = stabilise_filters(StabiliseStrength::Medium, "x.trf");
        assert_eq!(detect, "vidstabdetect=shakiness=6:accuracy=15:result=x.trf");
        assert!(transform.starts_with("vidstabtransform=input=x.trf:smoothing=20:optzoom=1"), "{transform}");
        let smoothing: Vec<u32> = StabiliseStrength::ALL.iter().map(|s| s.smoothing()).collect();
        assert!(smoothing.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn interpolation_multiplies_the_frame_rate() {
        assert_eq!(interpolate_filter(30.0, 2), "minterpolate=fps=60.000:mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1");
        assert!(interpolate_filter(23.976, 4).starts_with("minterpolate=fps=95.904:"));
        assert!(interpolate_filter(30.0, 99).starts_with("minterpolate=fps=240.000:"), "at most 8×");
    }
}

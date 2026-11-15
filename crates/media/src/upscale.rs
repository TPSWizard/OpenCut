//! AI upscaling with Real-ESRGAN's ncnn/Vulkan program
//! (`realesrgan-ncnn-vulkan`, BSD-3-Clause, fetched by
//! `scripts/get-realesrgan.sh` into `~/.opencut/tools/realesrgan`). It runs
//! on the GPU through Vulkan, one image at a time, so a video goes through
//! as frames: FFmpeg writes them out at the clip's frame rate, the program
//! upscales the folder, FFmpeg puts them back together with the audio.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codecs::{VideoCodec, pick_encoder};
use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// Largest side an upscaled picture may have (4K UHD).
pub const MAX_UPSCALED_SIDE: u32 = 3840;

/// Which Real-ESRGAN model to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpscaleModel {
    /// `realesrgan-x4plus`: the general model for photos and live action;
    /// 4× only, so 2× is made by scaling its result down.
    Photo,
    /// `realesr-animevideov3`: small and fast, made for animation; 2×, 3×
    /// or 4× directly.
    Animation,
}

impl UpscaleModel {
    pub fn id(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Animation => "animation",
        }
    }

    /// The model name the program takes.
    pub fn program_name(self) -> &'static str {
        match self {
            Self::Photo => "realesrgan-x4plus",
            Self::Animation => "realesr-animevideov3",
        }
    }

    /// The scale the program runs at for a wanted `scale`.
    pub fn native_scale(self, scale: u32) -> u32 {
        match self {
            Self::Photo => 4,
            Self::Animation => scale.clamp(2, 4),
        }
    }
}

/// The program: `OPENCUT_REALESRGAN`, else `~/.opencut/tools/realesrgan/`,
/// else on `PATH`.
pub fn locate_realesrgan() -> Option<PathBuf> {
    let name = if cfg!(windows) { "realesrgan-ncnn-vulkan.exe" } else { "realesrgan-ncnn-vulkan" };
    if let Some(path) = std::env::var_os("OPENCUT_REALESRGAN").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(path) = home.map(|home| home.join(".opencut").join("tools").join("realesrgan").join(name))
        && path.is_file()
    {
        return Some(path);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The output size for `scale` (2 to 4), or why it is too large.
pub fn upscaled_size(width: u32, height: u32, scale: u32) -> Result<(u32, u32), MediaError> {
    let scale = scale.clamp(2, 4);
    let (out_width, out_height) = (width * scale, height * scale);
    if out_width.max(out_height) > MAX_UPSCALED_SIDE {
        return Err(MediaError::Invalid(format!(
            "{scale}× of {width}×{height} is {out_width}×{out_height}, larger than {MAX_UPSCALED_SIDE} px; choose a smaller scale"
        )));
    }
    // Encoders want even sides.
    Ok((out_width & !1, out_height & !1))
}

fn run(mut command: Command, program: &str) -> Result<(), MediaError> {
    let result = command.output()?;
    if !result.status.success() {
        let message = crate::ffmpeg::last_stderr_line(&result.stderr);
        let message = if message.is_empty() { crate::ffmpeg::last_stderr_line(&result.stdout) } else { message };
        return Err(MediaError::ProgramFailed { program: program.to_owned(), message });
    }
    Ok(())
}

/// Runs the program over `input` (a file or folder) into `output`.
fn realesrgan(program: &Path, input: &Path, output: &Path, model: UpscaleModel, scale: u32) -> Result<(), MediaError> {
    let models = program.parent().map(|dir| dir.join("models")).unwrap_or_else(|| PathBuf::from("models"));
    let mut command = Command::new(program);
    command
        .arg("-i")
        .arg(input)
        .arg("-o")
        .arg(output)
        .arg("-m")
        .arg(&models)
        .args(["-n", model.program_name(), "-s", &model.native_scale(scale).to_string(), "-f", "png"]);
    run(command, "realesrgan-ncnn-vulkan")
}

/// Upscales a picture file to a PNG at `output`.
pub fn upscale_image(
    program: &Path,
    ffmpeg: &Ffmpeg,
    source: &Path,
    output: &Path,
    (width, height): (u32, u32),
    model: UpscaleModel,
    scale: u32,
) -> Result<(u32, u32), MediaError> {
    let (out_width, out_height) = upscaled_size(width, height, scale)?;
    let work = output.with_extension("upscale-work");
    std::fs::create_dir_all(&work)?;
    let result = (|| {
        // The program reads jpg/png/webp only; FFmpeg turns anything else into PNG.
        let input = work.join("in.png");
        let mut command = ffmpeg.command();
        command.arg("-y").arg("-i").arg(source).args(["-frames:v", "1"]).arg(&input);
        run(command, "ffmpeg")?;
        let upscaled = work.join("out.png");
        realesrgan(program, &input, &upscaled, model, scale)?;
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .arg("-i")
            .arg(&upscaled)
            .arg("-vf")
            .arg(format!("scale={out_width}:{out_height}:flags=lanczos"))
            .arg(output);
        run(command, "ffmpeg")
    })();
    std::fs::remove_dir_all(&work).ok();
    result.map(|()| (out_width, out_height))
}

/// Upscales a video file, or only `range` (start and length in seconds)
/// of it, to an H.264 MP4 at `output`, keeping its audio and frame rate.
/// Frames are written to a work folder next to `output`
/// (PNG, so they need disk space: about 1 MB a frame at 1080p) and removed
/// afterwards.
pub fn upscale_video(
    program: &Path,
    ffmpeg: &Ffmpeg,
    source: &Path,
    output: &Path,
    (width, height): (u32, u32),
    fps: f64,
    range: Option<(f64, f64)>,
    model: UpscaleModel,
    scale: u32,
) -> Result<(u32, u32), MediaError> {
    if !(fps.is_finite() && fps > 0.0) {
        return Err(MediaError::Invalid("The clip's frame rate is unknown".to_owned()));
    }
    let (out_width, out_height) = upscaled_size(width, height, scale)?;
    let work = output.with_extension("upscale-work");
    let (frames_in, frames_out) = (work.join("in"), work.join("out"));
    std::fs::create_dir_all(&frames_in)?;
    std::fs::create_dir_all(&frames_out)?;
    let partial = output.with_extension("partial.mp4");
    // The same span is read for the frames and for the sound.
    let span: Vec<String> = match range {
        Some((start, length)) => vec!["-ss".into(), format!("{:.6}", start.max(0.0)), "-t".into(), format!("{:.6}", length.max(0.01))],
        None => Vec::new(),
    };
    let result = (|| {
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .args(&span)
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0", "-vf", &format!("fps={fps}"), "-fps_mode", "cfr"])
            .arg(frames_in.join("%08d.png"));
        run(command, "ffmpeg")?;
        realesrgan(program, &frames_in, &frames_out, model, scale)?;
        let encoder = pick_encoder(ffmpeg, VideoCodec::H264, false)?;
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .args(["-framerate", &format!("{fps}")])
            .arg("-i")
            .arg(frames_out.join("%08d.png"))
            .args(&span)
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0", "-map", "1:a:0?", "-sn", "-dn"])
            .arg("-vf")
            .arg(format!("scale={out_width}:{out_height}:flags=lanczos"))
            .args(["-c:v", &encoder]);
        if encoder == "libx264" {
            command.args(["-preset", "medium", "-crf", "18"]);
        } else {
            command.args(["-b:v", "20M"]);
        }
        command
            .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart"])
            .arg(&partial);
        run(command, "ffmpeg")?;
        std::fs::rename(&partial, output)?;
        Ok(())
    })();
    std::fs::remove_dir_all(&work).ok();
    std::fs::remove_file(&partial).ok();
    result.map(|()| (out_width, out_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_stay_within_4k_and_even() {
        assert_eq!(upscaled_size(960, 540, 2).unwrap(), (1920, 1080));
        assert_eq!(upscaled_size(640, 360, 4).unwrap(), (2560, 1440));
        assert_eq!(upscaled_size(333, 201, 2).unwrap(), (666, 402));
        assert_eq!(upscaled_size(161, 91, 3).unwrap(), (482, 272));
        let error = upscaled_size(1920, 1080, 4).unwrap_err().to_string();
        assert!(error.contains("7680×4320"), "{error}");
        assert!(upscaled_size(1920, 1080, 2).is_ok(), "1080p doubles to exactly 4K");
    }

    #[test]
    fn models_run_at_their_native_scale() {
        assert_eq!(UpscaleModel::Photo.native_scale(2), 4, "x4plus only does 4×");
        assert_eq!(UpscaleModel::Animation.native_scale(2), 2);
        assert_eq!(UpscaleModel::Animation.native_scale(9), 4);
        assert_eq!(UpscaleModel::Photo.program_name(), "realesrgan-x4plus");
    }
}

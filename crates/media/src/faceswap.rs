//! Local face swap with FaceFusion (https://github.com/facefusion/facefusion,
//! OpenRAIL-AS), installed by the user, never by this repository: its
//! models carry their own licences (most are non-commercial or
//! research-only), which the person installing it accepts. FaceFusion runs
//! headless from its own folder (`python facefusion.py headless-run`), so
//! its models and `facefusion.ini` are found as usual.
//!
//! A video goes through as its span: FFmpeg cuts the part the clip plays,
//! FaceFusion swaps the face, and FFmpeg puts the span's own sound back so
//! the result always has it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codecs::{VideoCodec, pick_encoder};
use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// Where FaceFusion is: its folder (with `facefusion.py`) and the Python
/// that has its packages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceFusion {
    pub dir: PathBuf,
    pub python: PathBuf,
}

/// FaceFusion's face swapper models (its `--face-swapper-model` names).
/// HyperSwap is its default; GHOST is the one its docs list as Apache-2.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapModel {
    HyperSwap1a,
    HyperSwap1b,
    HyperSwap1c,
    Ghost2,
    InSwapper,
}

impl SwapModel {
    pub const ALL: [SwapModel; 5] = [Self::HyperSwap1a, Self::HyperSwap1b, Self::HyperSwap1c, Self::Ghost2, Self::InSwapper];

    pub fn program_name(self) -> &'static str {
        match self {
            Self::HyperSwap1a => "hyperswap_1a_256",
            Self::HyperSwap1b => "hyperswap_1b_256",
            Self::HyperSwap1c => "hyperswap_1c_256",
            Self::Ghost2 => "ghost_2_256",
            Self::InSwapper => "inswapper_128",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::HyperSwap1a => "HyperSwap 1a",
            Self::HyperSwap1b => "HyperSwap 1b",
            Self::HyperSwap1c => "HyperSwap 1c",
            Self::Ghost2 => "GHOST 2",
            Self::InSwapper => "InSwapper",
        }
    }
}

/// Which faces in the clip change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceChoice {
    /// The largest face in each frame.
    Largest,
    /// Every face.
    All,
    /// The face at this position from the left in the clip's first frame,
    /// followed through the clip (0 is the leftmost).
    FromLeft(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwapOptions {
    pub model: SwapModel,
    pub choice: FaceChoice,
    /// Also run FaceFusion's face enhancer (GFPGAN 1.4) over the swapped face.
    pub enhance: bool,
}

impl Default for SwapOptions {
    fn default() -> Self {
        Self { model: SwapModel::HyperSwap1a, choice: FaceChoice::Largest, enhance: false }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn python_in(env: &Path) -> PathBuf {
    if cfg!(windows) { env.join("python.exe") } else { env.join("bin").join("python") }
}

/// FaceFusion: the folder in `OPENCUT_FACEFUSION`, else
/// `~/.opencut/tools/facefusion`, else `~/facefusion`; its Python from
/// `OPENCUT_FACEFUSION_PYTHON`, else a conda environment called
/// `facefusion` (as FaceFusion's install guide makes) in Miniconda or
/// Anaconda, else a `venv` or `.venv` in the folder.
pub fn locate_facefusion() -> Option<FaceFusion> {
    let home = home();
    let dirs: Vec<PathBuf> = std::env::var_os("OPENCUT_FACEFUSION")
        .map(PathBuf::from)
        .into_iter()
        .chain(home.iter().flat_map(|home| [home.join(".opencut").join("tools").join("facefusion"), home.join("facefusion")]))
        .collect();
    let dir = dirs.into_iter().find(|dir| dir.join("facefusion.py").is_file())?;
    let mut pythons: Vec<PathBuf> = std::env::var_os("OPENCUT_FACEFUSION_PYTHON").map(PathBuf::from).into_iter().collect();
    if let Some(home) = &home {
        for root in ["miniconda3", "anaconda3", "Miniconda3", "Anaconda3"] {
            pythons.push(python_in(&home.join(root).join("envs").join("facefusion")));
            if cfg!(windows) {
                pythons.push(python_in(&home.join("AppData").join("Local").join(root).join("envs").join("facefusion")));
            }
        }
    }
    for venv in ["venv", ".venv"] {
        pythons.push(if cfg!(windows) { dir.join(venv).join("Scripts").join("python.exe") } else { dir.join(venv).join("bin").join("python") });
    }
    let python = pythons.into_iter().find(|path| path.is_file())?;
    Some(FaceFusion { dir, python })
}

/// FaceFusion's `headless-run` arguments (after the interpreter).
pub fn facefusion_args(face: &Path, target: &Path, output: &Path, options: &SwapOptions) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["facefusion.py".into(), "headless-run".into()];
    args.extend(["-s".into(), face.as_os_str().to_owned()]);
    args.extend(["-t".into(), target.as_os_str().to_owned()]);
    args.extend(["-o".into(), output.as_os_str().to_owned()]);
    args.push("--processors".into());
    args.push("face_swapper".into());
    if options.enhance {
        args.push("face_enhancer".into());
    }
    args.extend(["--face-swapper-model".into(), options.model.program_name().into()]);
    if options.enhance {
        args.extend(["--face-enhancer-model".into(), "gfpgan_1.4".into()]);
    }
    match options.choice {
        FaceChoice::Largest => args.extend(["--face-selector-mode", "one", "--face-selector-order", "large-small"].map(OsString::from)),
        FaceChoice::All => args.extend(["--face-selector-mode", "many"].map(OsString::from)),
        FaceChoice::FromLeft(position) => {
            args.extend(["--face-selector-mode", "reference", "--face-selector-order", "left-right"].map(OsString::from));
            args.extend(["--reference-face-position".into(), position.to_string().into(), "--reference-frame-number".into(), "0".into()]);
        }
    }
    args.extend(["--output-video-encoder", "libx264", "--output-video-quality", "90", "--log-level", "info"].map(OsString::from));
    args
}

/// Runs FaceFusion from its folder; a failure carries its last message.
fn run_facefusion(facefusion: &FaceFusion, args: &[OsString]) -> Result<(), MediaError> {
    let result = Command::new(&facefusion.python)
        .args(args)
        .current_dir(&facefusion.dir)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()?;
    if !result.status.success() {
        let message = crate::ffmpeg::last_stderr_line(&result.stderr);
        let message = if message.is_empty() { crate::ffmpeg::last_stderr_line(&result.stdout) } else { message };
        let code = result.status.code().map(|code| format!(" (exit {code})")).unwrap_or_default();
        let explained = match result.status.code() {
            Some(2) => "FaceFusion's checks failed: it needs Python 3.10 or later and curl, ffmpeg and ffprobe on PATH".to_owned(),
            Some(3) => "FaceFusion declined the content".to_owned(),
            _ => message,
        };
        return Err(MediaError::ProgramFailed { program: format!("FaceFusion{code}"), message: explained });
    }
    Ok(())
}

fn run(mut command: Command, program: &str) -> Result<(), MediaError> {
    let result = command.output()?;
    if !result.status.success() {
        return Err(MediaError::ProgramFailed { program: program.to_owned(), message: crate::ffmpeg::last_stderr_line(&result.stderr) });
    }
    Ok(())
}

fn made(path: &Path) -> bool {
    std::fs::metadata(path).map(|meta| meta.len() > 0).unwrap_or(false)
}

/// Swaps `face` onto the chosen face in a picture; `output` is a PNG.
pub fn swap_face_image(
    facefusion: &FaceFusion,
    ffmpeg: &Ffmpeg,
    face: &Path,
    source: &Path,
    output: &Path,
    options: &SwapOptions,
) -> Result<(), MediaError> {
    let work = output.with_extension("faceswap-work");
    std::fs::create_dir_all(&work)?;
    let result = (|| {
        // FaceFusion reads common picture formats; FFmpeg makes anything else a PNG.
        let target = work.join("target.png");
        let mut command = ffmpeg.command();
        command.arg("-y").arg("-i").arg(source).args(["-frames:v", "1"]).arg(&target);
        run(command, "ffmpeg")?;
        let swapped = work.join("swapped.png");
        run_facefusion(facefusion, &facefusion_args(face, &target, &swapped, options))?;
        if !made(&swapped) {
            return Err(MediaError::Invalid("FaceFusion made no picture: no face was found, or the content was declined".to_owned()));
        }
        std::fs::copy(&swapped, output)?;
        Ok(())
    })();
    std::fs::remove_dir_all(&work).ok();
    result
}

/// Swaps `face` onto the chosen face in a video, or only `range` (start and
/// length in seconds) of it, into an H.264 MP4 at `output` with the span's
/// own sound.
pub fn swap_face_video(
    facefusion: &FaceFusion,
    ffmpeg: &Ffmpeg,
    face: &Path,
    source: &Path,
    output: &Path,
    range: Option<(f64, f64)>,
    options: &SwapOptions,
) -> Result<(), MediaError> {
    let work = output.with_extension("faceswap-work");
    std::fs::create_dir_all(&work)?;
    let partial = output.with_extension("partial.mp4");
    let span: Vec<String> = match range {
        Some((start, length)) => vec!["-ss".into(), format!("{:.6}", start.max(0.0)), "-t".into(), format!("{:.6}", length.max(0.01))],
        None => Vec::new(),
    };
    let result = (|| {
        // The span, re-encoded so it starts on a frame of its own.
        let encoder = pick_encoder(ffmpeg, VideoCodec::H264, false)?;
        let target = work.join("target.mp4");
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .args(&span)
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn", "-c:v", &encoder]);
        if encoder == "libx264" {
            command.args(["-preset", "fast", "-crf", "14"]);
        } else {
            command.args(["-b:v", "20M"]);
        }
        command.args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]).arg(&target);
        run(command, "ffmpeg")?;

        let swapped = work.join("swapped.mp4");
        run_facefusion(facefusion, &facefusion_args(face, &target, &swapped, options))?;
        if !made(&swapped) {
            return Err(MediaError::Invalid("FaceFusion made no video: no face was found, or the content was declined".to_owned()));
        }

        // The swapped picture with the span's own sound.
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .arg("-i")
            .arg(&swapped)
            .arg("-i")
            .arg(&target)
            .args(["-map", "0:v:0", "-map", "1:a:0?", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart"])
            .arg(&partial);
        run(command, "ffmpeg")?;
        std::fs::rename(&partial, output)?;
        Ok(())
    })();
    std::fs::remove_dir_all(&work).ok();
    std::fs::remove_file(&partial).ok();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(args: &[OsString]) -> String {
        args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn builds_the_headless_command() {
        let args = facefusion_args(Path::new("face.jpg"), Path::new("t.mp4"), Path::new("o.mp4"), &SwapOptions::default());
        assert_eq!(
            joined(&args),
            "facefusion.py headless-run -s face.jpg -t t.mp4 -o o.mp4 --processors face_swapper --face-swapper-model hyperswap_1a_256 \
             --face-selector-mode one --face-selector-order large-small --output-video-encoder libx264 --output-video-quality 90 --log-level info"
        );
        let args = facefusion_args(
            Path::new("f.png"),
            Path::new("t.png"),
            Path::new("o.png"),
            &SwapOptions { model: SwapModel::Ghost2, choice: FaceChoice::FromLeft(1), enhance: true },
        );
        let text = joined(&args);
        assert!(text.contains("--processors face_swapper face_enhancer --face-swapper-model ghost_2_256 --face-enhancer-model gfpgan_1.4"), "{text}");
        assert!(text.contains("--face-selector-mode reference --face-selector-order left-right --reference-face-position 1 --reference-frame-number 0"), "{text}");
        let all = joined(&facefusion_args(Path::new("f"), Path::new("t"), Path::new("o"), &SwapOptions { choice: FaceChoice::All, ..Default::default() }));
        assert!(all.contains("--face-selector-mode many"), "{all}");
        assert_eq!(SwapModel::ALL.map(SwapModel::program_name).len(), 5);
    }
}

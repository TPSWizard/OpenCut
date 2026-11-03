use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};

use crate::error::MediaError;

/// Paths of the `ffmpeg` and `ffprobe` programs this process uses.
#[derive(Clone, Debug)]
pub struct Ffmpeg {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    version: String,
}

static LOCATED: OnceLock<Result<Arc<Ffmpeg>, MediaError>> = OnceLock::new();

impl Ffmpeg {
    /// Finds the programs once per process and caches the result.
    pub fn locate() -> Result<Arc<Ffmpeg>, MediaError> {
        match LOCATED.get_or_init(|| Self::search().map(Arc::new)) {
            Ok(found) => Ok(Arc::clone(found)),
            Err(_) => Err(MediaError::FfmpegNotFound),
        }
    }

    /// Uses the programs in `dir` without searching.
    pub fn in_dir(dir: &Path) -> Result<Ffmpeg, MediaError> {
        Self::try_dir(dir).ok_or(MediaError::FfmpegNotFound)
    }

    fn search() -> Result<Ffmpeg, MediaError> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = std::env::var_os("OPENCUT_FFMPEG_DIR") {
            candidates.push(PathBuf::from(dir));
        }
        candidates.extend(pinned_build_dirs());
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            candidates.push(dir.to_path_buf());
            candidates.push(dir.join("ffmpeg"));
        }
        for dir in candidates {
            if let Some(found) = Self::try_dir(&dir) {
                return Ok(found);
            }
        }
        // PATH, via the shell's own lookup.
        let ffmpeg = PathBuf::from(program_name("ffmpeg"));
        let ffprobe = PathBuf::from(program_name("ffprobe"));
        match version_of(&ffmpeg) {
            Some(version) if version_of(&ffprobe).is_some() => Ok(Ffmpeg {
                ffmpeg,
                ffprobe,
                version,
            }),
            _ => Err(MediaError::FfmpegNotFound),
        }
    }

    fn try_dir(dir: &Path) -> Option<Ffmpeg> {
        let ffmpeg = dir.join(program_name("ffmpeg"));
        let ffprobe = dir.join(program_name("ffprobe"));
        if !ffmpeg.is_file() || !ffprobe.is_file() {
            return None;
        }
        let version = version_of(&ffmpeg)?;
        Some(Ffmpeg {
            ffmpeg,
            ffprobe,
            version,
        })
    }

    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg
    }

    pub fn ffprobe_path(&self) -> &Path {
        &self.ffprobe
    }

    /// The first line of `ffmpeg -version`, e.g. `ffmpeg version 8.1.3 ...`.
    pub fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(&self.ffmpeg);
        command
            .arg("-hide_banner")
            .arg("-nostdin")
            .arg("-loglevel")
            .arg("error")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        hide_console_window(&mut command);
        command
    }

    pub(crate) fn probe_command(&self) -> Command {
        let mut command = Command::new(&self.ffprobe);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_console_window(&mut command);
        command
    }
}

fn program_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_owned()
    }
}

/// `.cache/media/<build>/bin` under the repository, when running from a
/// development checkout (`moon run media:setup` fills it).
fn pinned_build_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        // crates/media -> repository root
        roots.push(PathBuf::from(manifest_dir).join("..").join(".."));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    for root in roots {
        let cache = root.join(".cache").join("media");
        let Ok(entries) = std::fs::read_dir(&cache) else {
            continue;
        };
        for entry in entries.flatten() {
            let bin = entry.path().join("bin");
            if bin.is_dir() {
                dirs.push(bin);
            }
        }
    }
    dirs
}

fn version_of(program: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|line| line.trim().to_owned())
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

/// The last useful line of a program's stderr, for error messages.
pub(crate) fn last_stderr_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
        .unwrap_or("no output")
        .to_owned()
}

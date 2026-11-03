use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error(
        "FFmpeg was not found. Install it (https://ffmpeg.org/download.html) and put `ffmpeg` and `ffprobe` on PATH, or set OPENCUT_FFMPEG_DIR to the folder that holds them"
    )]
    FfmpegNotFound,
    #[error("{program} failed: {message}")]
    ProgramFailed { program: String, message: String },
    #[error("could not read {path}: {message}")]
    Unreadable { path: PathBuf, message: String },
    #[error("{path} has no {stream} stream")]
    NoStream { path: PathBuf, stream: &'static str },
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("could not decode image: {0}")]
    Image(String),
    #[error("{0}")]
    Invalid(String),
}

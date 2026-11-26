//! Audio-only export (row 96): a rendered mix written as WAV, AAC (in an
//! `.m4a`) or Opus (in an Ogg `.opus`).

use std::path::Path;

use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// An audio-only file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFileFormat {
    /// 16-bit PCM WAV, 48 kHz stereo.
    Wav,
    /// AAC-LC in an MPEG-4 audio file.
    Aac,
    /// Opus in an Ogg file.
    Opus,
}

impl AudioFileFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aac => "m4a",
            Self::Opus => "opus",
        }
    }

    /// The format a file name asks for (`.wav`, `.m4a` / `.aac`, `.opus` / `.ogg`).
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "wav" => Some(Self::Wav),
            "m4a" | "aac" => Some(Self::Aac),
            "opus" | "ogg" => Some(Self::Opus),
            _ => None,
        }
    }

    /// FFmpeg's output arguments (after the input), `bitrate_kbps` for the lossy ones.
    pub fn args(self, bitrate_kbps: u32) -> Vec<String> {
        let rate = format!("{}k", bitrate_kbps.clamp(32, 512));
        let args: Vec<&str> = match self {
            Self::Wav => vec!["-c:a", "pcm_s16le"],
            Self::Aac => vec!["-c:a", "aac", "-b:a", &rate, "-movflags", "+faststart"],
            Self::Opus => vec!["-c:a", "libopus", "-b:a", &rate],
        };
        let mut out: Vec<String> = vec!["-vn".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into()];
        out.extend(args.into_iter().map(str::to_owned));
        out
    }
}

/// Encodes `input` (any audio FFmpeg reads, here the rendered mix) to `output` as `format`.
pub fn encode_audio_file(ffmpeg: &Ffmpeg, input: &Path, output: &Path, format: AudioFileFormat, bitrate_kbps: u32) -> Result<(), MediaError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut command = ffmpeg.command();
    command.arg("-y").arg("-i").arg(input).args(format.args(bitrate_kbps)).arg(output);
    let result = command.output()?;
    if !result.status.success() {
        return Err(MediaError::ProgramFailed { program: "ffmpeg".to_owned(), message: String::from_utf8_lossy(&result.stderr).trim().to_owned() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_from_names() {
        assert_eq!(AudioFileFormat::from_path(Path::new("a.WAV")), Some(AudioFileFormat::Wav));
        assert_eq!(AudioFileFormat::from_path(Path::new("a.m4a")), Some(AudioFileFormat::Aac));
        assert_eq!(AudioFileFormat::from_path(Path::new("a.ogg")), Some(AudioFileFormat::Opus));
        assert_eq!(AudioFileFormat::from_path(Path::new("a.mp4")), None);
        assert!(AudioFileFormat::Opus.args(160).contains(&"libopus".to_owned()));
    }
}

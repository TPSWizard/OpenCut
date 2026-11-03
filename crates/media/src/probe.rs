use std::path::Path;

use serde::Deserialize;
use time::MediaTime;

use crate::error::MediaError;
use crate::ffmpeg::{Ffmpeg, last_stderr_line};
use crate::image_decode::{image_dimensions, is_image_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub duration: Option<MediaTime>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub has_audio: bool,
    pub has_video: bool,
    pub container: String,
}

#[derive(Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    nb_frames: Option<String>,
    disposition: Option<ProbeDisposition>,
}

#[derive(Deserialize)]
struct ProbeDisposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

/// Reads what a file is and how long it runs. Images are read with the
/// `image` crate; everything else is asked of `ffprobe`.
pub fn probe(ffmpeg: &Ffmpeg, path: &Path) -> Result<MediaInfo, MediaError> {
    if is_image_path(path) {
        let (width, height) = image_dimensions(path)?;
        return Ok(MediaInfo {
            kind: MediaKind::Image,
            duration: None,
            width: Some(width),
            height: Some(height),
            fps: None,
            has_audio: false,
            has_video: true,
            container: "image".to_owned(),
        });
    }

    let output = ffmpeg
        .probe_command()
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(MediaError::Unreadable {
            path: path.to_path_buf(),
            message: last_stderr_line(&output.stderr),
        });
    }
    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout).map_err(|error| {
        MediaError::ProgramFailed {
            program: "ffprobe".to_owned(),
            message: format!("unexpected output: {error}"),
        }
    })?;
    Ok(interpret(parsed, path))
}

fn interpret(parsed: ProbeOutput, path: &Path) -> MediaInfo {
    let video = parsed.streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video")
            && stream
                .disposition
                .as_ref()
                .map(|d| d.attached_pic == 0)
                .unwrap_or(true)
            // Cover art in audio files shows up as a video stream of one frame.
            && !matches!(stream.codec_name.as_deref(), Some("mjpeg" | "png") if stream.nb_frames.as_deref() == Some("1"))
    });
    let audio = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));

    let format_duration = parsed
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(parse_f64);
    let stream_duration = video
        .and_then(|stream| stream.duration.as_deref())
        .or_else(|| audio.and_then(|stream| stream.duration.as_deref()))
        .and_then(parse_f64);
    let duration = format_duration
        .or(stream_duration)
        .filter(|seconds| *seconds > 0.0)
        .and_then(MediaTime::from_seconds_f64);

    let fps = video.and_then(|stream| {
        stream
            .avg_frame_rate
            .as_deref()
            .and_then(parse_ratio)
            .or_else(|| stream.r_frame_rate.as_deref().and_then(parse_ratio))
    });

    let container = parsed
        .format
        .as_ref()
        .and_then(|format| format.format_name.clone())
        .unwrap_or_default();

    let kind = if video.is_some() {
        MediaKind::Video
    } else if audio.is_some() {
        MediaKind::Audio
    } else if is_image_path(path) {
        MediaKind::Image
    } else {
        MediaKind::Video
    };

    MediaInfo {
        kind,
        duration,
        width: video.and_then(|stream| stream.width),
        height: video.and_then(|stream| stream.height),
        fps: fps.filter(|value| *value > 0.0),
        has_audio: audio.is_some(),
        has_video: video.is_some(),
        container,
    }
}

fn parse_f64(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

fn parse_ratio(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let numerator: f64 = numerator.trim().parse().ok()?;
    let denominator: f64 = denominator.trim().parse().ok()?;
    (denominator != 0.0).then(|| numerator / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_ffprobe_json() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "codec_name": "h264", "width": 1280, "height": 720, "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001"},
                {"codec_type": "audio", "codec_name": "aac"}
            ],
            "format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "12.500000"}
        }"#;
        let parsed: ProbeOutput = serde_json::from_str(json).unwrap();
        let info = interpret(parsed, Path::new("clip.mp4"));
        assert_eq!(info.kind, MediaKind::Video);
        assert_eq!(info.width, Some(1280));
        assert!((info.fps.unwrap() - 29.97).abs() < 0.01);
        assert!(info.has_audio);
        assert_eq!(info.duration, MediaTime::from_seconds_f64(12.5));
    }

    #[test]
    fn audio_with_cover_art_is_audio() {
        let json = r#"{
            "streams": [
                {"codec_type": "audio", "codec_name": "mp3", "duration": "200.1"},
                {"codec_type": "video", "codec_name": "mjpeg", "nb_frames": "1", "width": 500, "height": 500, "disposition": {"attached_pic": 1}}
            ],
            "format": {"format_name": "mp3"}
        }"#;
        let parsed: ProbeOutput = serde_json::from_str(json).unwrap();
        let info = interpret(parsed, Path::new("song.mp3"));
        assert_eq!(info.kind, MediaKind::Audio);
        assert!(!info.has_video);
        assert_eq!(info.width, None);
        assert_eq!(info.duration, MediaTime::from_seconds_f64(200.1));
    }
}

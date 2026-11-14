//! Video codec choice for exports: which codecs each container takes, the
//! default, reading a target bitrate typed in Mbps, and which FFmpeg encoder
//! does the work. The web app has the same rules
//! (`apps/web/src/export/codecs.ts`); both test against
//! `fixtures/export-codecs/cases.json`.
//!
//! FFmpeg builds differ in the encoders they include, and a listed hardware
//! encoder only works with the matching GPU and driver, so encoders are
//! tried once with a few frames before they are used.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use crate::encode::EncodeFormat;
use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
    Vp9,
}

pub const MIN_BITRATE_MBPS: f64 = 0.2;
pub const MAX_BITRATE_MBPS: f64 = 400.0;

impl VideoCodec {
    pub const ALL: [VideoCodec; 4] = [Self::H264, Self::Hevc, Self::Av1, Self::Vp9];

    pub fn id(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::Av1 => "av1",
            Self::Vp9 => "vp9",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "HEVC (H.265)",
            Self::Av1 => "AV1",
            Self::Vp9 => "VP9",
        }
    }

    /// Reads a codec name as people write it ("AVC", "h.265", "HEVC").
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().replace('.', "").as_str() {
            "h264" | "avc" => Some(Self::H264),
            "h265" | "hevc" => Some(Self::Hevc),
            "av1" => Some(Self::Av1),
            "vp9" => Some(Self::Vp9),
            _ => None,
        }
    }

    /// Software encoders, best first.
    fn software_encoders(self) -> &'static [&'static str] {
        match self {
            Self::H264 => &["libx264"],
            Self::Hevc => &["libx265"],
            Self::Av1 => &["libsvtav1", "libaom-av1"],
            Self::Vp9 => &["libvpx-vp9"],
        }
    }

    /// Hardware encoders: NVIDIA, Intel, AMD, then Windows Media Foundation.
    fn hardware_encoders(self) -> &'static [&'static str] {
        match self {
            Self::H264 => &["h264_nvenc", "h264_qsv", "h264_amf", "h264_videotoolbox", "h264_mf"],
            Self::Hevc => &["hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_videotoolbox", "hevc_mf"],
            Self::Av1 => &["av1_nvenc", "av1_qsv", "av1_amf", "av1_mf"],
            Self::Vp9 => &["vp9_qsv"],
        }
    }
}

/// The codecs a container takes, its default first.
pub fn codecs_for(format: EncodeFormat) -> &'static [VideoCodec] {
    match format {
        EncodeFormat::Mp4 => &[VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1],
        EncodeFormat::Webm => &[VideoCodec::Vp9, VideoCodec::Av1],
    }
}

pub fn default_codec(format: EncodeFormat) -> VideoCodec {
    codecs_for(format)[0]
}

/// The codec to use: the one asked for when the container takes it, else the container's default.
pub fn resolve_codec(format: EncodeFormat, codec: Option<VideoCodec>) -> VideoCodec {
    codec.filter(|codec| codecs_for(format).contains(codec)).unwrap_or_else(|| default_codec(format))
}

/// Bits per second from a bitrate typed in Mbps, or `None` when it is not a number in range.
pub fn parse_bitrate_mbps(text: &str) -> Option<u64> {
    let cleaned = text.trim().replacen(',', ".", 1);
    let (whole, fraction) = cleaned.split_once('.').unwrap_or((&cleaned, ""));
    let digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || (cleaned.contains('.') && fraction.is_empty()) {
        return None;
    }
    let mbps: f64 = cleaned.parse().ok()?;
    if !(MIN_BITRATE_MBPS..=MAX_BITRATE_MBPS).contains(&mbps) {
        return None;
    }
    Some((mbps * 1_000_000.0).round() as u64)
}

/// Whether an encoder runs on the hardware it needs, not just which ones
/// a build lists.
pub fn is_hardware_encoder(name: &str) -> bool {
    !name.starts_with("lib")
}

type Cache<T> = OnceLock<Mutex<HashMap<PathBuf, T>>>;
static LISTED: Cache<BTreeSet<String>> = OnceLock::new();
static WORKS: Cache<HashMap<String, bool>> = OnceLock::new();

/// The video encoders this FFmpeg build lists (`ffmpeg -encoders`).
pub fn listed_encoders(ffmpeg: &Ffmpeg) -> BTreeSet<String> {
    let cache = LISTED.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(ffmpeg.ffmpeg_path()) {
        return found.clone();
    }
    let mut command = ffmpeg.command();
    command.arg("-encoders").stdout(Stdio::piped());
    let names: BTreeSet<String> = command
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split_whitespace();
                    let flags = parts.next()?;
                    let name = parts.next()?;
                    (flags.len() == 6 && flags.starts_with('V')).then(|| name.to_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    cache.lock().unwrap().insert(ffmpeg.ffmpeg_path().to_path_buf(), names.clone());
    names
}

/// Encodes three small frames with `encoder` and says whether it worked.
fn encoder_works(ffmpeg: &Ffmpeg, encoder: &str) -> bool {
    let cache = WORKS.get_or_init(Default::default);
    if let Some(&works) = cache.lock().unwrap().get(ffmpeg.ffmpeg_path()).and_then(|found| found.get(encoder)) {
        return works;
    }
    let pixel_format = if is_hardware_encoder(encoder) { "nv12" } else { "yuv420p" };
    let mut command = ffmpeg.command();
    command.args([
        "-f", "lavfi", "-i", "color=black:s=320x240:r=30:d=0.2", "-frames:v", "3", "-pix_fmt", pixel_format, "-c:v",
        encoder, "-f", "null", "-",
    ]);
    let works = command.output().map(|output| output.status.success()).unwrap_or(false);
    cache
        .lock()
        .unwrap()
        .entry(ffmpeg.ffmpeg_path().to_path_buf())
        .or_default()
        .insert(encoder.to_owned(), works);
    works
}

/// The FFmpeg encoder for `codec`: a working hardware one first when
/// `hardware` is asked for, else (or when none works) the best software one
/// the build has.
pub fn pick_encoder(ffmpeg: &Ffmpeg, codec: VideoCodec, hardware: bool) -> Result<String, MediaError> {
    let listed = listed_encoders(ffmpeg);
    if hardware {
        for name in codec.hardware_encoders() {
            if listed.contains(*name) && encoder_works(ffmpeg, name) {
                return Ok((*name).to_owned());
            }
        }
    }
    codec
        .software_encoders()
        .iter()
        .find(|name| listed.contains(**name))
        .map(|name| (*name).to_owned())
        .ok_or_else(|| {
            MediaError::Invalid(format!(
                "This FFmpeg cannot encode {}; it has none of {}",
                codec.label(),
                codec.software_encoders().join(", ")
            ))
        })
}

/// The codecs this FFmpeg can encode in software, which every export can use.
pub fn available_codecs(ffmpeg: &Ffmpeg) -> Vec<VideoCodec> {
    let listed = listed_encoders(ffmpeg);
    VideoCodec::ALL
        .into_iter()
        .filter(|codec| codec.software_encoders().iter().any(|name| listed.contains(*name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/export-codecs/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn format(id: &str) -> EncodeFormat {
        match id {
            "mp4" => EncodeFormat::Mp4,
            "webm" => EncodeFormat::Webm,
            other => panic!("format {other}"),
        }
    }

    fn codec(value: &Value) -> Option<VideoCodec> {
        value.as_str().map(|id| VideoCodec::ALL.into_iter().find(|codec| codec.id() == id).expect(id))
    }

    #[test]
    fn containers_defaults_and_labels_match_the_fixture() {
        let cases = fixture();
        for (id, spec) in cases["formats"].as_object().unwrap() {
            let ids: Vec<&str> = codecs_for(format(id)).iter().map(|codec| codec.id()).collect();
            let expected: Vec<&str> = spec["codecs"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{id}");
            assert_eq!(default_codec(format(id)).id(), spec["default"].as_str().unwrap());
        }
        for codec in VideoCodec::ALL {
            assert_eq!(cases["labels"][codec.id()].as_str(), Some(codec.label()));
        }
    }

    #[test]
    fn falls_back_to_the_containers_default() {
        for item in fixture()["resolve"].as_array().unwrap() {
            let resolved = resolve_codec(format(item["format"].as_str().unwrap()), codec(&item["codec"]));
            assert_eq!(Some(resolved), codec(&item["expect"]), "{item}");
        }
    }

    #[test]
    fn reads_codec_names_and_bitrates() {
        let cases = fixture();
        for item in cases["parseCodec"].as_array().unwrap() {
            assert_eq!(VideoCodec::parse(item["text"].as_str().unwrap()), codec(&item["expect"]), "{item}");
        }
        assert_eq!(cases["bitrateMbps"]["min"].as_f64(), Some(MIN_BITRATE_MBPS));
        assert_eq!(cases["bitrateMbps"]["max"].as_f64(), Some(MAX_BITRATE_MBPS));
        for item in cases["bitrateMbps"]["cases"].as_array().unwrap() {
            assert_eq!(parse_bitrate_mbps(item["text"].as_str().unwrap()), item["expect"].as_u64(), "{item}");
        }
    }
}

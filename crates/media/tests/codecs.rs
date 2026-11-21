//! Encodes a short clip with every codec this FFmpeg has, in each container
//! that takes it, and checks what came out. Skips (passes) without FFmpeg.

use std::path::Path;
use std::process::Command;

use media::{
    EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg, VideoCodec, available_codecs, codecs_for,
    is_hardware_encoder, video_encoder,
};

fn stream(ffmpeg: &Ffmpeg, path: &Path) -> String {
    let output = Command::new(ffmpeg.ffprobe_path())
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,codec_tag_string,width,height"])
        .args(["-of", "csv=p=0"])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn encode(ffmpeg: &Ffmpeg, settings: &EncodeSettings) {
    let mut encoder = Encoder::start(ffmpeg, settings).unwrap_or_else(|error| panic!("{settings:?}: {error}"));
    for index in 0..15u8 {
        let mut frame = vec![0u8; (settings.width * settings.height * 4) as usize];
        for (pixel, chunk) in frame.chunks_exact_mut(4).enumerate() {
            chunk[0] = (pixel % 256) as u8;
            chunk[1] = index * 12;
            chunk[3] = 255;
        }
        encoder.write_frame(&frame).unwrap();
    }
    encoder.finish().unwrap_or_else(|error| panic!("{settings:?}: {error}"));
}

fn settings(path: &Path, format: EncodeFormat, codec: VideoCodec) -> EncodeSettings {
    EncodeSettings {
        output: path.to_path_buf(),
        width: 320,
        height: 240,
        fps: 30.0,
        format,
        quality: EncodeQuality::Low,
        platform: None,
        codec: Some(codec),
        video_bitrate: None,
        hardware: false,
        duration_seconds: 0.5,
        audio: vec![],
        disclosure: None,
    }
}

#[test]
fn every_available_codec_encodes_in_each_container() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let available = available_codecs(&ffmpeg);
    eprintln!("codecs here: {available:?}");
    for format in [EncodeFormat::Mp4, EncodeFormat::Webm] {
        for &codec in codecs_for(format) {
            if !available.contains(&codec) {
                eprintln!("no software {codec:?} encoder; skipping it");
                continue;
            }
            let path = dir.path().join(format!("{}.{}", codec.id(), format.extension()));
            encode(&ffmpeg, &settings(&path, format, codec));
            let found = stream(&ffmpeg, &path);
            let name = match codec {
                VideoCodec::H264 => "h264",
                VideoCodec::Hevc => "hevc",
                VideoCodec::Av1 => "av1",
                VideoCodec::Vp9 => "vp9",
            };
            assert!(found.starts_with(&format!("{name},")), "{format:?} {codec:?}: {found}");
            assert!(found.contains("320,240"), "{found}");
            if codec == VideoCodec::Hevc {
                assert!(found.contains("hvc1"), "HEVC in MP4 is tagged hvc1 for Apple players: {found}");
            }
        }
    }

    // A target bitrate lands near the file size it implies.
    if available.contains(&VideoCodec::H264) {
        let path = dir.path().join("rate.mp4");
        let mut rated = settings(&path, EncodeFormat::Mp4, VideoCodec::H264);
        rated.video_bitrate = Some(2_000_000);
        encode(&ffmpeg, &rated);
        assert!(std::fs::metadata(&path).unwrap().len() > 20_000, "a 2 Mbps half second is not tiny");
    }
}

/// Hardware encoding where this machine has a working encoder; otherwise
/// the software one is used and the export still works.
#[test]
fn hardware_falls_back_to_software() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    for codec in [VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1] {
        if !available_codecs(&ffmpeg).contains(&codec) {
            continue;
        }
        let path = dir.path().join(format!("hw-{}.mp4", codec.id()));
        let mut wanted = settings(&path, EncodeFormat::Mp4, codec);
        wanted.hardware = true;
        let encoder = video_encoder(&ffmpeg, &wanted).unwrap();
        eprintln!("{codec:?} with hardware asked for: {encoder} (hardware: {})", is_hardware_encoder(&encoder));
        encode(&ffmpeg, &wanted);
        assert!(stream(&ffmpeg, &path).starts_with(codec.id()), "{codec:?}");
    }
}

//! Runs against the real FFmpeg programs. Skips (passes) when they are not
//! installed, so CI without FFmpeg still runs the unit tests.

use std::path::{Path, PathBuf};
use std::process::Command;

use media::{
    AudioMixClip, AudioReader, AudioSettings, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, GainEase, GainKey,
    Ffmpeg, MediaKind, VideoSource, decode_image, probe, thumbnail,
};

fn ffmpeg() -> Option<std::sync::Arc<Ffmpeg>> {
    match Ffmpeg::locate() {
        Ok(ffmpeg) => Some(ffmpeg),
        Err(_) => {
            eprintln!("ffmpeg not installed; skipping");
            None
        }
    }
}

/// A 3 second 320x240 test pattern with a 440 Hz tone.
fn synthetic_clip(ffmpeg: &Ffmpeg, dir: &Path) -> PathBuf {
    let path = dir.join("clip.mp4");
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&path)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "could not create the synthetic clip");
    path
}

#[test]
fn probes_decodes_and_encodes() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = synthetic_clip(&ffmpeg, dir.path());

    let info = probe(&ffmpeg, &clip).unwrap();
    assert_eq!(info.kind, MediaKind::Video);
    assert_eq!((info.width, info.height), (Some(320), Some(240)));
    assert!(info.has_audio);
    assert!((info.fps.unwrap() - 30.0).abs() < 0.01);
    let duration = info.duration.unwrap().to_seconds_f64();
    assert!((duration - 3.0).abs() < 0.2, "duration {duration}");

    // Random access through the forward decoder.
    let mut source = VideoSource::new(ffmpeg.clone(), &clip, (320, 240), 160, 30.0);
    let first = source.frame_at(0.0).unwrap().unwrap().clone();
    assert_eq!((first.width, first.height), (160, 120));
    assert_eq!(first.bgra.len(), 160 * 120 * 4);
    let later = source.frame_at(2.0).unwrap().unwrap().clone();
    assert!((later.time_seconds - 2.0).abs() < 0.04, "{}", later.time_seconds);
    assert_ne!(first.bgra, later.bgra, "the test pattern changes over time");
    let next = source.frame_at(2.0 + 1.0 / 30.0).unwrap().unwrap().clone();
    assert!(next.time_seconds > later.time_seconds);
    // Backwards forces a reopen.
    let back = source.frame_at(0.5).unwrap().unwrap().clone();
    assert!((back.time_seconds - 0.5).abs() < 0.04);
    // Past the end keeps the last frame.
    assert!(source.frame_at(10.0).unwrap().is_some());

    let thumb = thumbnail(&ffmpeg, &clip, 1.0, 64, (320, 240)).unwrap();
    assert_eq!((thumb.width, thumb.height), (64, 48));

    // Audio decodes to interleaved stereo floats.
    let mut audio = AudioReader::open(&ffmpeg, &clip, 1.0, AudioSettings::default()).unwrap();
    let mut buffer = vec![0f32; 4800 * 2];
    let read = audio.read(&mut buffer).unwrap();
    assert_eq!(read, buffer.len());
    let peak = buffer.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.05 && peak <= 1.0, "peak {peak}"); // lavfi sine defaults to amplitude 0.1

    // Encode a short clip with the tone mixed in, then probe the result.
    let output = dir.path().join("out.mp4");
    let settings = EncodeSettings {
        output: output.clone(),
        width: 160,
        height: 120,
        fps: 30.0,
        format: EncodeFormat::Mp4,
        quality: EncodeQuality::Medium,
        duration_seconds: 1.0,
        audio: vec![AudioMixClip {
            path: clip.clone(),
            source_start_seconds: 0.5,
            source_end_seconds: 1.5,
            timeline_start_seconds: 0.0,
            gain: 0.8,
            // A fade-in, so ffmpeg parses the keyframed volume expression.
            gain_keys: vec![
                GainKey { seconds: 0.0, db: -30.0, ease: GainEase::Smooth },
                GainKey { seconds: 0.5, db: -2.0, ease: GainEase::Linear },
            ],
        }],
    };
    let mut encoder = Encoder::start(&ffmpeg, &settings).unwrap();
    for index in 0..30 {
        let mut frame = vec![0u8; 160 * 120 * 4];
        for pixel in frame.chunks_exact_mut(4) {
            pixel[2] = (index * 8) as u8; // red ramps up
            pixel[3] = 255;
        }
        encoder.write_frame(&frame).unwrap();
    }
    assert_eq!(encoder.frames_written(), 30);
    let written = encoder.finish().unwrap();
    assert_eq!(written, output);
    let info = probe(&ffmpeg, &output).unwrap();
    assert_eq!((info.width, info.height), (Some(160), Some(120)));
    assert!(info.has_audio);
    assert!((info.duration.unwrap().to_seconds_f64() - 1.0).abs() < 0.15);

    // Images decode in-process.
    let png = dir.path().join("pic.png");
    let image = image::RgbaImage::from_fn(8, 4, |x, _| image::Rgba([x as u8 * 30, 0, 255, 255]));
    image.save(&png).unwrap();
    let frame = decode_image(&png).unwrap();
    assert_eq!((frame.width, frame.height), (8, 4));
    assert_eq!(&frame.bgra[0..4], &[255, 0, 0, 255], "stored as BGRA");
    let info = probe(&ffmpeg, &png).unwrap();
    assert_eq!(info.kind, MediaKind::Image);
}

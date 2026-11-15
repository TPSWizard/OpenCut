//! Real-ESRGAN upscaling against the real program and FFmpeg, on synthetic
//! pictures made here. Skips (passes) when either is missing; fetch the
//! program with scripts/get-realesrgan.sh.

use std::path::Path;
use std::process::Command;

use media::{Ffmpeg, UpscaleModel, VideoReader, locate_realesrgan, probe, upscale_image, upscale_video};

fn ffmpeg_run(ffmpeg: &Ffmpeg, args: &[&str], output: &Path) {
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}

fn decode(ffmpeg: &Ffmpeg, path: &Path) -> (u32, u32, Vec<Vec<u8>>) {
    let info = probe(ffmpeg, path).unwrap();
    let size = (info.width.unwrap(), info.height.unwrap());
    let mut reader = VideoReader::open(ffmpeg, path, 0.0, size.0, size, info.fps.unwrap_or(1.0)).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        frames.push(frame.bgra);
    }
    (size.0, size.1, frames)
}

/// Mean absolute difference between horizontal neighbours (green): higher
/// is crisper edges.
fn sharpness(width: u32, height: u32, bgra: &[u8]) -> f64 {
    let (w, h) = (width as usize, height as usize);
    let mut total = 0.0;
    for y in 0..h {
        for x in 1..w {
            total += (bgra[(y * w + x) * 4 + 1] as f64 - bgra[(y * w + x - 1) * 4 + 1] as f64).abs();
        }
    }
    total / ((w - 1) * h) as f64
}

#[test]
fn upscales_a_picture_more_crisply_than_resizing() {
    let (Ok(ffmpeg), Some(program)) = (Ffmpeg::locate(), locate_realesrgan()) else {
        eprintln!("Real-ESRGAN or FFmpeg missing; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("small.png");
    ffmpeg_run(&ffmpeg, &["-f", "lavfi", "-i", "testsrc2=s=160x120:r=1:d=1", "-frames:v", "1"], &source);
    let output = dir.path().join("big.png");
    let started = std::time::Instant::now();
    let size = upscale_image(&program, &ffmpeg, &source, &output, (160, 120), UpscaleModel::Photo, 2).unwrap();
    eprintln!("photo model, 160×120 → {size:?} in {:?}", started.elapsed());
    assert_eq!(size, (320, 240));
    let resized = dir.path().join("resized.png");
    ffmpeg_run(&ffmpeg, &["-i", source.to_str().unwrap(), "-vf", "scale=320:240:flags=bicubic"], &resized);
    let ai = media::decode_image(&output).unwrap();
    assert_eq!((ai.width, ai.height), (320, 240));
    let plain = media::decode_image(&resized).unwrap();
    let (crisp, soft) = (sharpness(ai.width, ai.height, &ai.bgra), sharpness(plain.width, plain.height, &plain.bgra));
    eprintln!("edge contrast: Real-ESRGAN {crisp:.2}, bicubic {soft:.2}");
    assert!(crisp > soft, "the model sharpens: {crisp} vs {soft}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3, "the work folder is removed");
}

#[test]
fn upscales_a_video_keeping_its_frames_and_sound() {
    let (Ok(ffmpeg), Some(program)) = (Ffmpeg::locate(), locate_realesrgan()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("clip.mp4");
    ffmpeg_run(
        &ffmpeg,
        &[
            "-f", "lavfi", "-i", "testsrc2=s=160x90:r=10:d=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
        ],
        &source,
    );
    let output = dir.path().join("out").join("clip-2x.mp4");
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    let started = std::time::Instant::now();
    let size = upscale_video(&program, &ffmpeg, &source, &output, (160, 90), 10.0, None, UpscaleModel::Animation, 2).unwrap();
    eprintln!("animation model, 20 frames 160×90 → {size:?} in {:?}", started.elapsed());
    assert_eq!(size, (320, 180));
    let info = probe(&ffmpeg, &output).unwrap();
    assert_eq!((info.width, info.height), (Some(320), Some(180)));
    assert!(info.has_audio, "the sound is kept");
    assert!((info.fps.unwrap() - 10.0).abs() < 0.01, "{:?}", info.fps);
    let seconds = info.duration.unwrap().to_seconds_f64();
    assert!((seconds - 2.0).abs() < 0.15, "{seconds}");
    let (_, _, frames) = decode(&ffmpeg, &output);
    assert_eq!(frames.len(), 20);
    let leftovers: Vec<_> = std::fs::read_dir(output.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers.len(), 1, "no frames or partial file left: {leftovers:?}");
}

/// Only the part a clip plays is upscaled: 0.5 s from 0.8 s is 5 frames.
#[test]
fn upscales_only_the_span_asked_for() {
    let (Ok(ffmpeg), Some(program)) = (Ffmpeg::locate(), locate_realesrgan()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("clip.mp4");
    ffmpeg_run(
        &ffmpeg,
        &[
            "-f", "lavfi", "-i", "testsrc2=s=160x90:r=10:d=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
        ],
        &source,
    );
    let output = dir.path().join("span.mp4");
    upscale_video(&program, &ffmpeg, &source, &output, (160, 90), 10.0, Some((0.8, 0.5)), UpscaleModel::Animation, 2).unwrap();
    let info = probe(&ffmpeg, &output).unwrap();
    let seconds = info.duration.unwrap().to_seconds_f64();
    assert!((seconds - 0.5).abs() < 0.1, "{seconds}");
    assert!(info.has_audio);
    assert_eq!(decode(&ffmpeg, &output).2.len(), 5);
}

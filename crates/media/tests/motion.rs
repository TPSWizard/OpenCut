//! Stabilisation and in-between frames against the real FFmpeg, on
//! synthetic clips made here. Skips (passes) without FFmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;

use media::{Ffmpeg, StabiliseStrength, VideoReader, can_interpolate, can_stabilise, interpolate_video, probe, stabilise_video};

fn lavfi(ffmpeg: &Ffmpeg, path: &Path, video: &str, filter: Option<&str>, audio: bool) {
    let mut command = Command::new(ffmpeg.ffmpeg_path());
    command.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", video]);
    if audio {
        command.args(["-f", "lavfi", "-i", "sine=frequency=330:duration=3"]);
    }
    if let Some(filter) = filter {
        command.args(["-vf", filter]);
    }
    command.args(["-c:v", "libx264", "-crf", "16", "-pix_fmt", "yuv420p"]);
    if audio {
        command.args(["-c:a", "aac", "-shortest"]);
    }
    assert!(command.arg(path).status().unwrap().success());
}

/// One still frame of FFmpeg's test pattern seen through a 320 × 240
/// window that jumps around by up to 24 px every frame: camera shake with
/// nothing else moving. (A repeating texture would fool the tracker.)
fn shaky_clip(ffmpeg: &Ffmpeg, dir: &Path) -> PathBuf {
    let still = dir.join("still.png");
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=400x300:r=1:d=1", "-frames:v", "1"])
        .arg(&still)
        .status()
        .unwrap();
    assert!(status.success());
    let path = dir.join("shaky.mp4");
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-loop", "1", "-framerate", "30", "-t", "3", "-i"])
        .arg(&still)
        .args(["-f", "lavfi", "-i", "sine=frequency=330:duration=3"])
        .args(["-vf", "crop=320:240:x='40+24*sin(n*1.3)':y='30+20*cos(n*1.7)'"])
        .args(["-c:v", "libx264", "-crf", "16", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    path
}

fn frames(ffmpeg: &Ffmpeg, path: &Path) -> Vec<Vec<u8>> {
    let info = probe(ffmpeg, path).unwrap();
    let size = (info.width.unwrap(), info.height.unwrap());
    let mut reader = VideoReader::open(ffmpeg, path, 0.0, size.0, size, info.fps.unwrap()).unwrap();
    let mut out = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        out.push(frame.bgra);
    }
    out
}

/// The average camera move between consecutive frames, in pixels: for
/// each pair, the whole-pixel shift (on a 4× smaller copy, searched up to
/// ±40 px) that best lines the middle of one frame up with the next.
fn shake(frames: &[Vec<u8>], width: usize, height: usize) -> f64 {
    let small = |frame: &Vec<u8>| -> Vec<f64> {
        let (w, h) = (width / 4, height / 4);
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for dy in 0..4 {
                    for dx in 0..4 {
                        sum += frame[((y * 4 + dy) * width + x * 4 + dx) * 4 + 1] as f64;
                    }
                }
                out[y * w + x] = sum / 16.0;
            }
        }
        out
    };
    let (w, h) = (width as i64 / 4, height as i64 / 4);
    let reach = 10i64;
    let mut total = 0.0;
    for pair in frames.windows(2) {
        let (a, b) = (small(&pair[0]), small(&pair[1]));
        let mut best = (f64::MAX, 0i64, 0i64);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let mut cost = 0.0;
                for y in h / 4..h * 3 / 4 {
                    for x in w / 4..w * 3 / 4 {
                        let (bx, by) = (x + dx, y + dy);
                        cost += (a[(y * w + x) as usize] - b[(by * w + bx) as usize]).abs();
                    }
                }
                if cost < best.0 {
                    best = (cost, dx, dy);
                }
            }
        }
        total += (((best.1 * best.1 + best.2 * best.2) as f64).sqrt()) * 4.0;
    }
    total / (frames.len() - 1) as f64
}

#[test]
fn stabilising_takes_most_of_the_shake_out() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = shaky_clip(&ffmpeg, dir.path());
    let before = shake(&frames(&ffmpeg, &source), 320, 240);
    let output = dir.path().join("out").join("stable.mp4");
    if !can_stabilise(&ffmpeg) {
        let error = stabilise_video(&ffmpeg, &source, &output, StabiliseStrength::Strong).unwrap_err();
        assert!(error.to_string().contains("vid.stab"), "{error}");
        eprintln!("no vid.stab in this FFmpeg; checked the message only");
        return;
    }
    stabilise_video(&ffmpeg, &source, &output, StabiliseStrength::Strong).unwrap();
    let info = probe(&ffmpeg, &output).unwrap();
    assert_eq!((info.width, info.height), (Some(320), Some(240)));
    assert!(info.has_audio, "the audio is kept");
    let source_seconds = probe(&ffmpeg, &source).unwrap().duration.unwrap().to_seconds_f64();
    assert!((info.duration.unwrap().to_seconds_f64() - source_seconds).abs() < 0.1, "same length");
    let after = shake(&frames(&ffmpeg, &output), 320, 240);
    eprintln!("camera move per frame {before:.1} px -> {after:.1} px");
    assert!(after < before * 0.35, "camera move per frame {before:.1} px -> {after:.1} px");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("out")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers.len(), 1, "no transforms file or partial left: {leftovers:?}");
}

/// The bar moves 8 px a frame at 10 fps; doubled, the new frame between two
/// originals shows it about halfway.
#[test]
fn in_between_frames_land_halfway() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    if !can_interpolate(&ffmpeg) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bar.mp4");
    lavfi(
        &ffmpeg,
        &source,
        "color=c=black:s=320x96:r=10:d=2[bg];color=c=white:s=24x48:r=10:d=2[bar];[bg][bar]overlay=x='40+80*t':y=24",
        None,
        false,
    );
    let output = dir.path().join("smooth.mp4");
    let fps = interpolate_video(&ffmpeg, &source, &output, 10.0, 2).unwrap();
    assert_eq!(fps, 20.0);
    let info = probe(&ffmpeg, &output).unwrap();
    assert!((info.fps.unwrap() - 20.0).abs() < 0.01, "{:?}", info.fps);
    let frames = frames(&ffmpeg, &output);
    assert!((36..=41).contains(&frames.len()), "about twice the 20 frames: {}", frames.len());
    let centre = |frame: &Vec<u8>| {
        let (mut sum, mut weight) = (0.0, 0.0);
        for y in 30..66 {
            for x in 0..320 {
                let value = frame[(y * 320 + x) * 4 + 1] as f64;
                sum += value * x as f64;
                weight += value;
            }
        }
        sum / weight
    };
    for index in [10usize, 20] {
        let (a, middle, b) = (centre(&frames[index - 1]), centre(&frames[index]), centre(&frames[index + 1]));
        eprintln!("frame {index}: {a:.1} {middle:.1} {b:.1}");
        assert!(a < middle && middle < b, "frame {index} lies between its neighbours");
        assert!((middle - (a + b) / 2.0).abs() < 2.0, "frame {index} is about halfway");
    }
}

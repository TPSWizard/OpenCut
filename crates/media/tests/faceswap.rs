//! The local face swap pipeline around FaceFusion, with a stand-in
//! FaceFusion (a Python script that records its arguments and copies the
//! target, without sound, to the output) and synthetic media made here.
//! Skips (passes) when FFmpeg or Python is missing. A real FaceFusion is
//! checked by hand; it is never downloaded by this repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use media::{FaceChoice, FaceFusion, Ffmpeg, SwapModel, SwapOptions, probe, swap_face_image, swap_face_video};

const FAKE: &str = r#"
import json, os, shutil, subprocess, sys
args = sys.argv[1:]
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "args.json"), "w") as f:
    json.dump({"args": args, "cwd": os.getcwd()}, f)
here = os.path.dirname(os.path.abspath(__file__))
mode = open(os.path.join(here, "mode.txt")).read().strip() if os.path.exists(os.path.join(here, "mode.txt")) else "copy"
if mode == "fail":
    print("[FACEFUSION.CORE] Processing to video failed", file=sys.stderr)
    sys.exit(1)
if mode == "declined":
    sys.exit(3)
if mode == "nothing":
    sys.exit(0)
target = args[args.index("-t") + 1]
output = args[args.index("-o") + 1]
if target.endswith(".mp4"):
    # Like a swapper that writes picture only: drop the sound.
    subprocess.run([os.environ["FAKE_FFMPEG"], "-hide_banner", "-loglevel", "error", "-y", "-i", target, "-an", "-c:v", "copy", output], check=True)
else:
    shutil.copy(target, output)
"#;

fn python() -> Option<PathBuf> {
    for name in ["python", "python3"] {
        if let Ok(output) = Command::new(name).arg("-c").arg("import sys; print(sys.executable)").output()
            && output.status.success()
        {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn setup() -> Option<(std::sync::Arc<Ffmpeg>, FaceFusion, tempfile::TempDir)> {
    let (Ok(ffmpeg), Some(python)) = (Ffmpeg::locate(), python()) else {
        eprintln!("FFmpeg or Python missing; skipping");
        return None;
    };
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("facefusion");
    std::fs::create_dir_all(&install).unwrap();
    std::fs::write(install.join("facefusion.py"), FAKE).unwrap();
    // SAFETY: every test sets the same value; nothing else reads it.
    unsafe { std::env::set_var("FAKE_FFMPEG", ffmpeg.ffmpeg_path()) };
    Some((ffmpeg, FaceFusion { dir: install, python }, dir))
}

fn make(ffmpeg: &Ffmpeg, args: &[&str], output: &Path) {
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}

fn recorded(facefusion: &FaceFusion) -> (Vec<String>, String) {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(facefusion.dir.join("args.json")).unwrap()).unwrap();
    let args = value["args"].as_array().unwrap().iter().map(|arg| arg.as_str().unwrap().to_owned()).collect();
    (args, value["cwd"].as_str().unwrap().to_owned())
}

#[test]
fn swaps_a_video_span_and_keeps_its_sound() {
    let Some((ffmpeg, facefusion, dir)) = setup() else { return };
    let source = dir.path().join("clip.mp4");
    make(
        &ffmpeg,
        &["-f", "lavfi", "-i", "testsrc2=s=320x240:r=25:d=4", "-f", "lavfi", "-i", "sine=frequency=440:duration=4", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"],
        &source,
    );
    let face = dir.path().join("face.png");
    make(&ffmpeg, &["-f", "lavfi", "-i", "color=c=orange:s=64x64:d=1", "-frames:v", "1"], &face);
    let output = dir.path().join("out").join("swapped.mp4");
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    let options = SwapOptions { model: SwapModel::HyperSwap1c, choice: FaceChoice::FromLeft(1), enhance: false };
    swap_face_video(&facefusion, &ffmpeg, &face, &source, &output, Some((1.0, 2.0)), &options).unwrap();

    let info = probe(&ffmpeg, &output).unwrap();
    let duration = info.duration.unwrap().to_seconds_f64();
    assert!((duration - 2.0).abs() < 0.15, "span length {duration}");
    assert!(info.has_audio, "the span's sound is put back");
    let (args, cwd) = recorded(&facefusion);
    assert_eq!(args[0], "headless-run", "facefusion.py headless-run");
    assert!(args.windows(2).any(|pair| pair == ["--face-swapper-model", "hyperswap_1c_256"]), "{args:?}");
    assert!(args.windows(2).any(|pair| pair == ["--reference-face-position", "1"]), "{args:?}");
    assert_eq!(Path::new(&cwd).canonicalize().unwrap(), facefusion.dir.canonicalize().unwrap(), "runs from its own folder");
    // The work folder is cleaned up.
    assert!(!output.with_extension("faceswap-work").exists());
}

#[test]
fn swaps_a_picture() {
    let Some((ffmpeg, facefusion, dir)) = setup() else { return };
    let source = dir.path().join("photo.jpg");
    make(&ffmpeg, &["-f", "lavfi", "-i", "testsrc2=s=200x150:r=1:d=1", "-frames:v", "1"], &source);
    let face = dir.path().join("face.png");
    make(&ffmpeg, &["-f", "lavfi", "-i", "color=c=orange:s=64x64:d=1", "-frames:v", "1"], &face);
    let output = dir.path().join("swapped.png");
    swap_face_image(&facefusion, &ffmpeg, &face, &source, &output, &SwapOptions::default()).unwrap();
    let image = media::decode_image(&output).unwrap();
    assert_eq!((image.width, image.height), (200, 150));
}

#[test]
fn reports_failures_plainly() {
    let Some((ffmpeg, facefusion, dir)) = setup() else { return };
    let source = dir.path().join("photo.png");
    make(&ffmpeg, &["-f", "lavfi", "-i", "testsrc2=s=64x64:r=1:d=1", "-frames:v", "1"], &source);
    let face = source.clone();
    let output = dir.path().join("o.png");
    for (mode, expected) in [("fail", "Processing to video failed"), ("declined", "declined"), ("nothing", "no face was found")] {
        std::fs::write(facefusion.dir.join("mode.txt"), mode).unwrap();
        let error = swap_face_image(&facefusion, &ffmpeg, &face, &source, &output, &SwapOptions::default()).unwrap_err().to_string();
        assert!(error.contains(expected), "{mode}: {error}");
    }
}

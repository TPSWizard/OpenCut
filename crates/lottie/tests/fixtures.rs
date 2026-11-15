//! The shared cases in `fixtures/lottie/cases.json`, read natively.

use std::path::PathBuf;

use lottie::{Animation, LottieError, is_lottie};
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lottie")
}

fn cases() -> Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("cases.json")).unwrap()).unwrap()
}

fn f(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

#[test]
fn reads_both_files_and_draws_the_expected_pixels() {
    let cases = cases();
    let tolerance = f(&cases["tolerance"]) as i32;
    for file in cases["files"].as_array().unwrap() {
        let file = file.as_str().unwrap();
        let bytes = std::fs::read(fixtures().join(file)).unwrap();
        assert!(is_lottie(&bytes), "{file}");
        let mut animation = Animation::from_bytes(&bytes).unwrap_or_else(|error| panic!("{file}: {error}"));
        let info = animation.info();
        let expected = &cases["info"];
        assert_eq!(info.width, f(&expected["width"]) as u32);
        assert_eq!(info.height, f(&expected["height"]) as u32);
        assert_eq!(info.frame_rate, f(&expected["frameRate"]));
        assert_eq!(info.in_frame, f(&expected["inFrame"]));
        assert_eq!(info.out_frame, f(&expected["outFrame"]));
        assert_eq!(info.duration_seconds(), f(&expected["durationSeconds"]));
        for case in cases["frameAt"].as_array().unwrap() {
            assert_eq!(info.frame_at(f(&case["seconds"])), f(&case["frame"]), "{case}");
        }

        let size = &cases["render"];
        let (width, height) = (f(&size["width"]) as u32, f(&size["height"]) as u32);
        let check = |rgba: &[u8], width: u32, case: &Value| {
            let (x, y) = (f(&case["x"]) as usize, f(&case["y"]) as usize);
            let at = (y * width as usize + x) * 4;
            let got = &rgba[at..at + 4];
            let want: Vec<i32> = case["rgba"].as_array().unwrap().iter().map(|v| f(v) as i32).collect();
            let close = got.iter().zip(&want).all(|(&g, &w)| (g as i32 - w).abs() <= tolerance);
            // Fully transparent pixels may carry any colour.
            let both_clear = want[3] == 0 && got[3] as i32 <= tolerance;
            assert!(close || both_clear, "{file} {case}: got {got:?}");
        };
        for case in cases["pixels"].as_array().unwrap() {
            let rgba = animation.render_rgba(f(&case["frame"]), width, height);
            assert_eq!(rgba.len(), (width * height * 4) as usize);
            check(&rgba, width, case);
        }
        let scaled = &cases["scaled"];
        let (sw, sh) = (f(&scaled["width"]) as u32, f(&scaled["height"]) as u32);
        let rgba = animation.render_rgba(f(&scaled["frame"]), sw, sh);
        assert_eq!(rgba.len(), (sw * sh * 4) as usize);
        check(&rgba, sw, scaled);
    }
}

#[test]
fn draws_mattes_strokes_and_gradients() {
    let features = &cases()["features"];
    let tolerance = f(&features["tolerance"]) as i32;
    let bytes = std::fs::read(fixtures().join(features["file"].as_str().unwrap())).unwrap();
    let mut animation = Animation::from_bytes(&bytes).unwrap();
    let width = f(&features["width"]) as u32;
    let rgba = animation.render_rgba(f(&features["frame"]), width, f(&features["height"]) as u32);
    for case in features["pixels"].as_array().unwrap() {
        let at = ((f(&case["y"]) as usize) * width as usize + f(&case["x"]) as usize) * 4;
        let got = &rgba[at..at + 4];
        let want: Vec<i32> = case["rgba"].as_array().unwrap().iter().map(|v| f(v) as i32).collect();
        let close = got.iter().zip(&want).all(|(&g, &w)| (g as i32 - w).abs() <= tolerance);
        let both_clear = want[3] == 0 && got[3] as i32 <= tolerance;
        assert!(close || both_clear, "{case}: got {got:?}");
    }
}

#[test]
fn refuses_what_is_not_lottie_or_too_much() {
    for case in cases()["rejects"].as_array().unwrap() {
        let json = case["json"].as_str().unwrap();
        let error = Animation::from_bytes(json.as_bytes()).expect_err(json);
        let kind = case["kind"].as_str().unwrap();
        match kind {
            "notLottie" => assert!(matches!(error, LottieError::NotLottie(_)), "{json}: {error}"),
            "limits" => assert!(matches!(error, LottieError::Limits(_)), "{json}: {error}"),
            other => panic!("unknown kind {other}"),
        }
        if kind == "notLottie" {
            assert!(!is_lottie(json.as_bytes()));
        }
    }
    assert!(!is_lottie(b"PK\x03\x04 not really a zip"));
    assert_eq!(Animation::from_bytes(&vec![b' '; lottie::MAX_FILE_BYTES + 1]).unwrap_err(), LottieError::TooLarge);
}

#[test]
fn draws_quickly_enough_for_preview() {
    let bytes = std::fs::read(fixtures().join("bounce.json")).unwrap();
    let mut animation = Animation::from_bytes(&bytes).unwrap();
    let started = std::time::Instant::now();
    for frame in 0..60 {
        animation.render_rgba(frame as f64, 512, 512);
    }
    let per_frame = started.elapsed() / 60;
    eprintln!("512 px frame: {per_frame:?}");
    assert!(per_frame.as_millis() < 100, "{per_frame:?}");
}

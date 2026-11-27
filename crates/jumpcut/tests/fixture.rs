//! Checks the crate against `fixtures/jumpcut/jumpcut.json`, written by an
//! independent Python reference: the jump cuts found on three timelines,
//! bridge plans, and the motion and in-between frames for four synthetic
//! picture pairs (rebuilt here from the same formulas).

use jumpcut::{CutClip, find_jump_cuts, in_between, motion_between, plan_bridge};
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/jumpcut/jumpcut.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn f(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

#[test]
fn finds_the_jump_cuts_the_reference_finds() {
    let data = fixture();
    for timeline in data["timelines"].as_array().unwrap() {
        let clips: Vec<CutClip> = timeline["clips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|clip| CutClip {
                id: clip["id"].as_str().unwrap().to_owned(),
                track: clip["track"].as_str().unwrap().to_owned(),
                media: clip["media"].as_str().unwrap().to_owned(),
                start: f(&clip["start"]),
                end: f(&clip["end"]),
                source_start: f(&clip["sourceStart"]),
                source_end: f(&clip["sourceEnd"]),
            })
            .collect();
        let cuts = find_jump_cuts(&clips, f(&timeline["fps"]));
        let want = timeline["cuts"].as_array().unwrap();
        assert_eq!(cuts.len(), want.len(), "{}", timeline["name"]);
        for (cut, want) in cuts.iter().zip(want) {
            assert_eq!(cut.before as u64, want["before"].as_u64().unwrap());
            assert_eq!(cut.after as u64, want["after"].as_u64().unwrap());
            assert!((cut.time - f(&want["time"])).abs() < 1e-12);
            assert!((cut.source_gap - f(&want["sourceGap"])).abs() < 1e-12);
        }
    }
}

#[test]
fn plans_bridges_as_the_reference_does() {
    let data = fixture();
    for case in data["plans"].as_array().unwrap() {
        let plan = plan_bridge(
            f(&case["cut"]),
            f(&case["beforeStart"]),
            f(&case["afterEnd"]),
            f(&case["fps"]),
            case["frames"].as_u64().unwrap() as usize,
        );
        let want = &case["plan"];
        match plan {
            None => assert!(want.is_null(), "{case}"),
            Some(plan) => {
                assert_eq!(plan.frames as u64, want["frames"].as_u64().unwrap(), "{case}");
                for (got, key) in [(plan.start, "start"), (plan.duration, "duration"), (plan.from_time, "fromTime"), (plan.to_time, "toTime")] {
                    assert!((got - f(&want[key])).abs() < 1e-12, "{key} {case}");
                }
                let alphas: Vec<f64> = want["alphas"].as_array().unwrap().iter().map(f).collect();
                assert_eq!(plan.alphas.len(), alphas.len());
                for (a, b) in plan.alphas.iter().zip(&alphas) {
                    assert!((a - b).abs() < 1e-12);
                }
            }
        }
    }
}

fn pat(x: f64, y: f64, c: f64) -> f64 {
    0.5 + 0.2 * (0.31 * x + 0.17 * y + 1.3 * c).sin()
        + 0.15 * (0.11 * x - 0.23 * y + 2.1 + 0.7 * c).sin()
        + 0.1 * (0.05 * x + 0.37 * y + c).cos()
}

fn back(x: f64, y: f64, c: f64) -> f64 {
    0.45 + 0.12 * (0.19 * x - 0.13 * y + 0.9 * c).sin() + 0.08 * (0.27 * y + 0.4 * c).cos()
}

fn q8(value: f64) -> u8 {
    (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn pair(case: &Value, t: f64) -> Vec<u8> {
    let (w, h) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
    let mut out = vec![0u8; w * h * 4];
    let kind = case["kind"].as_str().unwrap();
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f64, y as f64);
            for c in 0..3 {
                let cf = c as f64;
                let value = match kind {
                    "shift" => {
                        let (dx, dy) = (f(&case["shift"][0]), f(&case["shift"][1]));
                        let zoom = case.get("zoom").map(f).unwrap_or(1.0);
                        pat((xf - t * dx) / zoom, (yf - t * dy) / zoom, cf)
                    }
                    "head" => {
                        let (cx, cy) = (
                            f(&case["from"][0]) + t * (f(&case["to"][0]) - f(&case["from"][0])),
                            f(&case["from"][1]) + t * (f(&case["to"][1]) - f(&case["from"][1])),
                        );
                        let (rx, ry) = (f(&case["radii"][0]), f(&case["radii"][1]));
                        if ((xf - cx) / rx).powi(2) + ((yf - cy) / ry).powi(2) <= 1.0 {
                            0.15 + pat(xf - cx, yf - cy, cf)
                        } else {
                            back(xf, yf, cf)
                        }
                    }
                    _ => pat(xf, yf, cf),
                };
                out[(y * w + x) * 4 + c] = q8(value);
            }
            out[(y * w + x) * 4 + 3] = 255;
        }
    }
    out
}

fn mae(a: &[u8], b: &[u8], w: usize, h: usize, margin: usize) -> f64 {
    let mut total = 0.0;
    let mut n = 0.0;
    for y in margin..h - margin {
        for x in margin..w - margin {
            for c in 0..3 {
                let i = (y * w + x) * 4 + c;
                total += (f64::from(a[i]) - f64::from(b[i])).abs();
                n += 1.0;
            }
        }
    }
    total / n
}

#[test]
fn makes_the_in_between_frames_the_reference_makes() {
    let data = fixture();
    for case in data["morphs"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (w, h) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
        let (p, q) = (pair(case, 0.0), pair(case, 1.0));
        let field = motion_between(&p, &q, w, h).unwrap();
        assert_eq!(field.width as u64, case["work"]["width"].as_u64().unwrap(), "{name}");
        assert_eq!(field.height as u64, case["work"]["height"].as_u64().unwrap(), "{name}");
        assert_eq!(field.scale as u64, case["work"]["scale"].as_u64().unwrap(), "{name}");
        for sample in case["flow"].as_array().unwrap() {
            let (x, y) = (sample[0].as_u64().unwrap() as usize, sample[1].as_u64().unwrap() as usize);
            let k = y * field.width + x;
            assert!((field.u[k] - f(&sample[2])).abs() < 1e-6, "{name} u at {x},{y}: {} vs {}", field.u[k], sample[2]);
            assert!((field.v[k] - f(&sample[3])).abs() < 1e-6, "{name} v at {x},{y}: {} vs {}", field.v[k], sample[3]);
        }
        for frame in case["frames"].as_array().unwrap() {
            let alpha = f(&frame["alpha"]);
            let out = in_between(&p, &q, w, h, &field, alpha);
            for sample in frame["samples"].as_array().unwrap() {
                let (x, y) = (sample[0].as_u64().unwrap() as usize, sample[1].as_u64().unwrap() as usize);
                for c in 0..4 {
                    let want = sample[2 + c].as_i64().unwrap();
                    let got = i64::from(out[(y * w + x) * 4 + c]);
                    assert!((got - want).abs() <= 1, "{name} α {alpha} at {x},{y} channel {c}: {got} vs {want}");
                }
            }
            // Closer to the true in-between picture than a crossfade is.
            let truth = if case["kind"] == "still" { p.clone() } else { pair(case, alpha) };
            let error = mae(&out, &truth, w, h, 8);
            assert!((error - f(&frame["morphError"])).abs() < 0.05, "{name}: error {error} vs {}", frame["morphError"]);
            let crossfade = f(&frame["crossfadeError"]);
            if crossfade > 0.0 {
                assert!(error < crossfade * 0.25, "{name}: {error} is not well under the crossfade's {crossfade}");
            } else {
                assert_eq!(out, p, "{name}: nothing moved, so the frame is the picture");
            }
        }
    }
}

#[test]
fn rejects_mismatched_pictures() {
    assert!(motion_between(&[0; 16], &[0; 12], 2, 2).is_none());
    assert!(motion_between(&[0; 4], &[0; 4], 1, 1).is_none());
}

/// How long a 1080p pair takes (run with `--release -- --ignored`).
#[test]
#[ignore]
fn time_a_full_hd_pair() {
    let (w, h) = (1920usize, 1080usize);
    let make = |shift: f64| -> Vec<u8> {
        let mut out = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    out[(y * w + x) * 4 + c] = q8(pat((x as f64 - shift) / 6.0, y as f64 / 6.0, c as f64));
                }
            }
        }
        out
    };
    let (p, q) = (make(0.0), make(30.0));
    let started = std::time::Instant::now();
    let field = motion_between(&p, &q, w, h).unwrap();
    let motion = started.elapsed();
    let frame = in_between(&p, &q, w, h, &field, 0.5);
    eprintln!("motion {motion:?}, one frame {:?}, median {:.1} px", started.elapsed() - motion, field.median());
    assert_eq!(frame.len(), p.len());
}

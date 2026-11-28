//! Checks the crate against `fixtures/stabilise/stabilise.json`, written by
//! an independent Python reference.

use serde_json::Value;
use stabilise::{CameraPath, Similarity, Strength, fit_similarity, frame_step};

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/stabilise/stabilise.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn similarity(value: &Value) -> Similarity {
    let v: Vec<f64> = value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    Similarity { a: v[0], b: v[1], tx: v[2], ty: v[3] }
}

fn close(a: &Similarity, b: &Similarity, tolerance: f64) -> bool {
    (a.a - b.a).abs() < tolerance && (a.b - b.b).abs() < tolerance && (a.tx - b.tx).abs() < tolerance && (a.ty - b.ty).abs() < tolerance
}

fn bytes(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()
}

#[test]
fn fits_match_the_reference() {
    for case in fixture()["fits"].as_array().unwrap() {
        let points: Vec<[f64; 4]> = case["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| std::array::from_fn(|i| p[i].as_f64().unwrap()))
            .collect();
        let got = fit_similarity(&points, case["sigmaFloor"].as_f64().unwrap()).unwrap();
        let want = similarity(&case["fit"]);
        assert!(close(&got, &want, 1e-9), "{}: {got:?} vs {want:?}", case["name"]);
        assert!(close(&got, &similarity(&case["truth"]), 2e-4), "{}: near the truth", case["name"]);
    }
}

#[test]
fn frame_steps_match_the_reference() {
    for case in fixture()["steps"].as_array().unwrap() {
        let (w, h) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
        let (p, q) = (bytes(&case["p"]), bytes(&case["q"]));
        let got = frame_step(&p, &q, w, h);
        let want = (!case["step"].is_null()).then(|| similarity(&case["step"]));
        match (got, want) {
            (Some(got), Some(want)) => {
                assert!(close(&got, &want, 1e-6), "{}: {got:?} vs {want:?}", case["name"]);
                let truth = similarity(&case["truth"]);
                // Within a twentieth of a pixel of the true movement.
                assert!((got.tx - truth.tx).abs() * w as f64 <= 0.05 && (got.ty - truth.ty).abs() * w as f64 <= 0.05, "{}", case["name"]);
                assert!((got.angle() - truth.angle()).abs() < 2e-4 && (got.scale() - truth.scale()).abs() < 2e-4, "{}", case["name"]);
            }
            (None, None) => {}
            other => panic!("{}: {other:?}", case["name"]),
        }
        // BGRA gives the same.
        let swap = |v: &[u8]| v.chunks(4).flat_map(|c| [c[2], c[1], c[0], c[3]]).collect::<Vec<u8>>();
        assert_eq!(frame_step(&swap(&p), &swap(&q), w, h), got);
    }
}

#[test]
fn corrections_and_lookups_match_the_reference() {
    let data = fixture();
    let path_case = &data["path"];
    let steps: Vec<Option<Similarity>> =
        path_case["steps"].as_array().unwrap().iter().map(|s| (!s.is_null()).then(|| similarity(s))).collect();
    let path = CameraPath {
        media_id: "clip-1".into(),
        start: 1.5,
        rate: 30.0,
        aspect: path_case["aspect"].as_f64().unwrap(),
        steps,
    };
    for case in path_case["corrections"].as_array().unwrap() {
        let strength = Strength::parse(case["strength"].as_str().unwrap()).unwrap();
        let got = path.corrections(strength);
        assert!((got.zoom - case["zoom"].as_f64().unwrap()).abs() < 1e-9, "{strength:?} zoom {}", got.zoom);
        assert!((got.needed - case["needed"].as_f64().unwrap()).abs() < 1e-9, "{strength:?}");
        let frames = case["frames"].as_array().unwrap();
        assert_eq!(got.frames.len(), frames.len());
        for (k, (g, w)) in got.frames.iter().zip(frames).enumerate() {
            assert!(close(g, &similarity(w), 1e-9), "{strength:?} frame {k}: {g:?} vs {w}");
        }
    }

    let text = path_case["encoded"].as_str().unwrap();
    assert_eq!(path.encode(), text, "encodes as the reference does");
    let parsed = CameraPath::parse(text).unwrap();
    assert_eq!((parsed.media_id.as_str(), parsed.frames()), ("clip-1", path.frames()));
    let medium = parsed.corrections(Strength::Medium);
    for lookup in path_case["lookups"].as_array().unwrap() {
        let seconds = lookup["seconds"].as_f64().unwrap();
        let got = medium.at(seconds);
        let want = similarity(&lookup["correction"]);
        assert!(close(&got, &want, 1e-9), "at {seconds}: {got:?} vs {want:?}");
    }
}

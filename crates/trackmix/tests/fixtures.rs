//! The crate against `fixtures/trackmix/cases.json`, written by an
//! independent reference.

use serde_json::Value;
use trackmix::{TrackSettings, pan_clip_and_track, track_mix};

#[test]
fn matches_the_reference() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/trackmix/cases.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let tracks: Vec<TrackSettings> = serde_json::from_value(case["tracks"].clone()).unwrap();
        let mix = track_mix(&tracks);
        for (got, want) in mix.iter().zip(case["mix"].as_array().unwrap()) {
            assert_eq!(got.audible, want["audible"].as_bool().unwrap(), "{name}");
            assert!((got.gain - want["gain"].as_f64().unwrap()).abs() < 1e-12, "{name}: {got:?} vs {want}");
            assert!((got.pan - want["pan"].as_f64().unwrap()).abs() < 1e-12, "{name}");
        }
    }
    for pan in fixture["pans"].as_array().unwrap() {
        let input = pan["in"].as_array().unwrap();
        let (left, right) = pan_clip_and_track(
            pan["clipPan"].as_f64().unwrap(),
            pan["trackPan"].as_f64().unwrap(),
            input[0].as_f64().unwrap(),
            input[1].as_f64().unwrap(),
        );
        let out = pan["out"].as_array().unwrap();
        assert!((left - out[0].as_f64().unwrap()).abs() < 1e-12 && (right - out[1].as_f64().unwrap()).abs() < 1e-12, "{pan}");
    }
}

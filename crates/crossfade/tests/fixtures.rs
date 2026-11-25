//! The crate against `fixtures/crossfade/cases.json`, written by an
//! independent reference.

use crossfade::{ClipAudio, Join, PlanError, TrackClip, crossfade_track, joins, plan};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    clips: Vec<TrackClip>,
    joins: Vec<Join>,
    played: Vec<Option<ClipAudio>>,
    plan: Option<PlanCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanCase {
    index: usize,
    ticks: i64,
    result: serde_json::Value,
    after_joins: Option<Vec<Join>>,
}

#[test]
fn matches_the_reference() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/crossfade/cases.json");
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(fixture.cases.len() >= 20);
    for case in &fixture.cases {
        assert_eq!(joins(&case.clips), case.joins, "{}: joins", case.name);
        let played = crossfade_track(&case.clips);
        assert_eq!(played.len(), case.played.len(), "{}", case.name);
        for (got, want) in played.iter().zip(&case.played) {
            match (got, want) {
                (Some(got), Some(want)) => {
                    assert_eq!(
                        (got.start, got.duration, got.trim_start, got.trim_end, got.fade_in_curve, got.fade_out_curve),
                        (want.start, want.duration, want.trim_start, want.trim_end, want.fade_in_curve, want.fade_out_curve),
                        "{}",
                        case.name
                    );
                    assert!((got.fade_in - want.fade_in).abs() < 1e-12, "{}: fade in", case.name);
                    assert!((got.fade_out - want.fade_out).abs() < 1e-12, "{}: fade out", case.name);
                }
                (None, None) => {}
                _ => panic!("{}: {got:?} vs {want:?}", case.name),
            }
        }
        let Some(plan_case) = &case.plan else { continue };
        let result = plan(&case.clips, plan_case.index, plan_case.ticks);
        match plan_case.result.get("error").and_then(|error| error.as_str()) {
            Some(error) => {
                let want: PlanError = serde_json::from_value(serde_json::Value::String(error.to_owned())).unwrap();
                assert_eq!(result, Err(want), "{}", case.name);
            }
            None => {
                let got = serde_json::to_value(result.unwrap()).unwrap();
                assert_eq!(got, plan_case.result, "{}: plan", case.name);
                assert!(plan_case.after_joins.is_some(), "{}", case.name);
            }
        }
    }
}

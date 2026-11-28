//! Checks the crate against `fixtures/interchange/cases.json`, written by an
//! independent Python reference (its readers use the stdlib `json` and
//! `xml.etree` parsers).

use interchange::{Format, Rate, Timeline, edl, fcpxml, frames_to_timecode, otio, read, timecode_to_frames};
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/interchange/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn rate(value: &Value) -> Rate {
    serde_json::from_value(value.clone()).unwrap()
}

fn timeline(value: &Value) -> Timeline {
    serde_json::from_value(value.clone()).unwrap()
}

fn line_diff(got: &str, want: &str) -> String {
    for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
        if g != w {
            return format!("line {}:\n got  {g:?}\n want {w:?}", i + 1);
        }
    }
    format!("lengths {} vs {}", got.lines().count(), want.lines().count())
}

#[test]
fn timecodes_times_and_reels_match_the_reference() {
    let data = fixture();
    for case in data["timecodes"].as_array().unwrap() {
        let (frames, r) = (case["frames"].as_i64().unwrap(), rate(&case["rate"]));
        let tc = case["tc"].as_str().unwrap();
        assert_eq!(frames_to_timecode(frames, r), tc, "{case}");
        assert_eq!(timecode_to_frames(tc, r), Some(frames), "{case}");
    }
    for case in data["times"].as_array().unwrap() {
        assert_eq!(fcpxml::time_text(case["frames"].as_i64().unwrap(), rate(&case["rate"])), case["text"].as_str().unwrap(), "{case}");
    }
    for case in data["urls"].as_array().unwrap() {
        let url = interchange::path_to_url(case["path"].as_str().unwrap());
        assert_eq!(url, case["url"].as_str().unwrap(), "{case}");
        assert_eq!(interchange::url_to_path(&url), case["back"].as_str().unwrap(), "{case}");
    }
    for case in data["reels"].as_array().unwrap() {
        assert_eq!(edl::reel_name(case["file"].as_str().unwrap()), case["reel"].as_str().unwrap(), "{case}");
    }
}

#[test]
fn writers_match_the_reference() {
    for case in fixture()["cases"].as_array().unwrap() {
        let tl = timeline(&case["timeline"]);
        let written: Value = serde_json::from_str(&otio::write(&tl)).unwrap();
        assert_eq!(written, case["otio"], "{}: OTIO", tl.name);
        let fx = fcpxml::write(&tl);
        let want = case["fcpxml"].as_str().unwrap();
        assert!(fx == want, "{}: FCPXML {}", tl.name, line_diff(&fx, want));
        let e = edl::write(&tl);
        let want = case["edl"].as_str().unwrap();
        assert!(e == want, "{}: EDL {}", tl.name, line_diff(&e, want));
    }
}

#[test]
fn our_files_read_back_as_the_reference_reads_them() {
    for case in fixture()["cases"].as_array().unwrap() {
        let tl = timeline(&case["timeline"]);
        for (format, text) in [
            (Format::Otio, otio::write(&tl)),
            (Format::Fcpxml, fcpxml::write(&tl)),
            (Format::Edl, edl::write(&tl)),
        ] {
            let got = read(&text, format, tl.rate).unwrap();
            let want = timeline(&case["back"][format.extension()]);
            assert_eq!(got, want, "{}: {format:?}", tl.name);
        }
    }
}

#[test]
fn files_from_other_editors_read_as_the_reference_reads_them() {
    for case in fixture()["foreign"].as_array().unwrap() {
        let format = Format::parse(case["format"].as_str().unwrap()).unwrap();
        let got = read(case["text"].as_str().unwrap(), format, rate(&case["rate"])).unwrap();
        let want = timeline(&case["timeline"]);
        assert_eq!(got, want, "{}", case["name"]);
    }
}

#[test]
fn sniffs_formats_and_refuses_junk() {
    assert_eq!(Format::sniff("cut.EDL", ""), Some(Format::Edl));
    assert_eq!(Format::sniff("x", "  {\"OTIO_SCHEMA\": \"Timeline.1\"}"), Some(Format::Otio));
    assert_eq!(Format::sniff("x", "<?xml version=\"1.0\"?><fcpxml/>"), Some(Format::Fcpxml));
    assert_eq!(Format::sniff("x", "TITLE: a\n"), Some(Format::Edl));
    let rate = Rate::new(30, 1);
    assert!(read("not json", Format::Otio, rate).is_err());
    assert!(read("{\"OTIO_SCHEMA\": \"Clip.2\"}", Format::Otio, rate).is_err());
    assert!(read("<x/>", Format::Fcpxml, rate).is_err());
    assert!(read("TITLE: nothing\n", Format::Edl, rate).is_err());
}

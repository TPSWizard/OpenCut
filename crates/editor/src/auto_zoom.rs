//! Auto zoom punch-ins: small zooms in and out on the speaker at sentence
//! starts and on emphasis. A clip's punch-ins are params worked out as it
//! plays, on top of its keyframes (the web app's `src/auto-zoom/zoom.ts`,
//! checked against the same `fixtures/auto-zoom/cases.json`):
//!
//! - `zoom.punches`: `"start-end@scale;…"` in clip seconds.
//! - `zoom.focusX` / `zoom.focusY`: the point zoomed into, as a share of the
//!   clip's frame from its centre (-0.5 to 0.5).
//! - `zoom.ramp`: seconds each zoom takes in and out, at most half the punch.

use crate::{CaptionWord, Params};

pub const ZOOM_FOCUS_Y_DEFAULT: f64 = -0.1;
pub const ZOOM_RAMP_DEFAULT: f64 = 0.15;
pub const ZOOM_RAMP_MAX: f64 = 1.0;
pub const ZOOM_SCALE_MAX: f64 = 3.0;
pub const SENTENCE_GAP_SECONDS: f64 = 0.7;
pub const MIN_PUNCH_SECONDS: f64 = 0.8;
pub const TAIL_SECONDS: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomPunch {
    pub start: f64,
    pub end: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipZoom {
    pub punches: Vec<ZoomPunch>,
    pub focus_x: f64,
    pub focus_y: f64,
    pub ramp: f64,
}

/// `(id, normal zoom, emphasis zoom)` of each strength.
pub const ZOOM_STRENGTHS: &[(&str, f64, f64)] = &[("subtle", 1.08, 1.15), ("medium", 1.15, 1.3), ("strong", 1.25, 1.5)];

/// A spoken word in clip seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoomWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub keyword: bool,
}

fn round(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

pub fn format_punches(punches: &[ZoomPunch]) -> String {
    punches
        .iter()
        .map(|punch| format!("{:.2}-{:.2}@{:.3}", round(punch.start, 2), round(punch.end, 2), round(punch.scale, 3)))
        .collect::<Vec<_>>()
        .join(";")
}

/// The punches of a `zoom.punches` string; malformed or empty entries are skipped.
pub fn parse_punches(text: &str) -> Vec<ZoomPunch> {
    let mut punches: Vec<ZoomPunch> = text
        .split(';')
        .filter_map(|part| {
            let (span, scale) = part.trim().split_once('@')?;
            // The first '-' after the first character splits start from end.
            let span = span.trim();
            let split = span.char_indices().skip(1).find(|(_, c)| *c == '-')?.0;
            let start: f64 = span[..split].trim().parse().ok()?;
            let end: f64 = span[split + 1..].trim().parse().ok()?;
            let scale: f64 = scale.trim().parse().ok()?;
            (start.is_finite() && end.is_finite() && scale.is_finite() && end > start && scale > 0.0)
                .then_some(ZoomPunch { start, end, scale: scale.min(ZOOM_SCALE_MAX) })
        })
        .collect();
    punches.sort_by(|a, b| a.start.total_cmp(&b.start));
    punches
}

/// The clip's punch-ins, or `None` when it has none.
pub fn read_clip_zoom(params: &Params) -> Option<ClipZoom> {
    let punches = parse_punches(&params.text("zoom.punches", ""));
    if punches.is_empty() {
        return None;
    }
    Some(ClipZoom {
        punches,
        focus_x: params.number("zoom.focusX", 0.0).clamp(-0.5, 0.5),
        focus_y: params.number("zoom.focusY", ZOOM_FOCUS_Y_DEFAULT).clamp(-0.5, 0.5),
        ramp: params.number("zoom.ramp", ZOOM_RAMP_DEFAULT).clamp(0.0, ZOOM_RAMP_MAX),
    })
}

fn smoothstep(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The zoom (1 = none) at `seconds` into the clip.
pub fn zoom_at(zoom: &ClipZoom, seconds: f64) -> f64 {
    for punch in &zoom.punches {
        if seconds < punch.start || seconds >= punch.end {
            continue;
        }
        let ramp = zoom.ramp.min((punch.end - punch.start) / 2.0);
        let amount = if ramp <= 0.0 {
            1.0
        } else {
            smoothstep((seconds - punch.start) / ramp).min(smoothstep((punch.end - seconds) / ramp))
        };
        return 1.0 + (punch.scale - 1.0) * amount;
    }
    1.0
}

/// The offset that keeps the focus point still when a frame drawn
/// `width` × `height` (before the zoom), turned by `rotate` degrees, is
/// zoomed by `scale`.
pub fn zoom_offset(focus: (f64, f64), scale: f64, width: f64, height: f64, rotate: f64) -> (f64, f64) {
    let local_x = (1.0 - scale) * focus.0 * width;
    let local_y = (1.0 - scale) * focus.1 * height;
    let (sin, cos) = rotate.to_radians().sin_cos();
    (local_x * cos - local_y * sin, local_x * sin + local_y * cos)
}

fn ends_sentence(text: &str) -> bool {
    text.trim().trim_end_matches(['"', '\'', '\u{201d}', '\u{2019}', ')']).ends_with(['.', '!', '?'])
}

/// Punch-ins from the clip's spoken words (see the web app's `planPunches`).
pub fn plan_punches(words: &[ZoomWord], duration: f64, strength: &str) -> Vec<ZoomPunch> {
    let (_, normal, emphasis) =
        ZOOM_STRENGTHS.iter().find(|(id, _, _)| *id == strength).copied().unwrap_or(ZOOM_STRENGTHS[1]);
    let mut sorted: Vec<&ZoomWord> =
        words.iter().filter(|word| !word.text.trim().is_empty() && word.end > 0.0 && word.start < duration).collect();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut sentences: Vec<Vec<&ZoomWord>> = Vec::new();
    for word in sorted {
        let starts = match sentences.last().and_then(|sentence| sentence.last()) {
            Some(previous) => ends_sentence(&previous.text) || word.start - previous.end >= SENTENCE_GAP_SECONDS,
            None => true,
        };
        if starts {
            sentences.push(vec![word]);
        } else {
            sentences.last_mut().expect("a sentence").push(word);
        }
    }
    let mut punches: Vec<ZoomPunch> = Vec::new();
    for (index, sentence) in sentences.iter().enumerate() {
        let emphasised = sentence
            .iter()
            .any(|word| word.keyword || word.text.chars().any(char::is_numeric) || word.text.contains('!'));
        let scale = if emphasised {
            emphasis
        } else if index % 2 == 1 {
            normal
        } else {
            1.0
        };
        if scale == 1.0 {
            continue;
        }
        let start = sentence[0].start.max(0.0);
        let end = match sentences.get(index + 1) {
            Some(next) => next[0].start,
            None => sentence[sentence.len() - 1].end + TAIL_SECONDS,
        }
        .min(duration);
        if let Some(last) = punches.last_mut()
            && last.scale == scale
            && (last.end - start).abs() < 1e-6
        {
            last.end = end;
            continue;
        }
        punches.push(ZoomPunch { start, end, scale });
    }
    punches.retain(|punch| punch.end - punch.start >= MIN_PUNCH_SECONDS);
    punches
}

/// The words of the captions over a clip, in its seconds: every text
/// element with timed words that overlaps `start..end` on the timeline.
pub fn clip_words(project: &crate::Project, start: time::MediaTime, end: time::MediaTime) -> Vec<ZoomWord> {
    let mut words = Vec::new();
    for track in &project.tracks {
        for element in track.elements.iter().filter(|element| !element.words.is_empty()) {
            if element.end_time() <= start || element.start_time >= end {
                continue;
            }
            let offset = (element.start_time - start).to_seconds_f64();
            words.extend(element.words.iter().map(|word: &CaptionWord| ZoomWord {
                text: word.text.clone(),
                start: offset + word.start.to_seconds_f64(),
                end: offset + word.end.to_seconds_f64(),
                keyword: word.keyword,
            }));
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_the_shared_cases_like_the_web() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/auto-zoom/cases.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 3);
        for case in cases {
            let words: Vec<ZoomWord> = case["words"]
                .as_array()
                .unwrap()
                .iter()
                .map(|word| ZoomWord {
                    text: word["text"].as_str().unwrap().to_owned(),
                    start: word["start"].as_f64().unwrap(),
                    end: word["end"].as_f64().unwrap(),
                    keyword: word["keyword"].as_bool().unwrap_or(false),
                })
                .collect();
            let planned = plan_punches(&words, case["duration"].as_f64().unwrap(), case["strength"].as_str().unwrap());
            assert_eq!(format_punches(&planned), case["punches"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn parses_and_eases() {
        assert_eq!(parse_punches("2-3@1.2; nope ;0.5-1@1.1;4-3@2"), vec![
            ZoomPunch { start: 0.5, end: 1.0, scale: 1.1 },
            ZoomPunch { start: 2.0, end: 3.0, scale: 1.2 },
        ]);
        let zoom = ClipZoom { punches: vec![ZoomPunch { start: 1.0, end: 3.0, scale: 1.4 }], focus_x: 0.0, focus_y: 0.0, ramp: 0.2 };
        assert_eq!(zoom_at(&zoom, 0.99), 1.0);
        assert!((zoom_at(&zoom, 1.1) - 1.2).abs() < 1e-9);
        assert!((zoom_at(&zoom, 2.0) - 1.4).abs() < 1e-9);
        assert_eq!(zoom_at(&zoom, 3.0), 1.0);
        let (x, y) = zoom_offset((0.25, -0.1), 2.0, 400.0, 200.0, 0.0);
        assert!((x + 100.0).abs() < 1e-9 && (y - 20.0).abs() < 1e-9);
        let mut params = Params::new();
        params.set("zoom.punches", "0-1@1.5").set("zoom.focusX", 3.0);
        let read = read_clip_zoom(&params).unwrap();
        assert_eq!((read.focus_x, read.focus_y, read.ramp), (0.5, -0.1, 0.15));
        assert!(read_clip_zoom(&Params::new()).is_none());
    }
}

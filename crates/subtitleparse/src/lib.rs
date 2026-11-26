//! Subtitle import (row 95): SRT and WebVTT parsed the same by both
//! editors (the web through WASM) into cues with their text and times in
//! seconds.
//!
//! - **SRT**: blocks of an optional number, `hh:mm:ss,mmm --> hh:mm:ss,mmm`
//!   and text lines. A dot instead of the comma is read too.
//! - **WebVTT**: the `WEBVTT` header, `NOTE`, `STYLE` and `REGION` blocks
//!   skipped, optional cue identifiers, `mm:ss.ttt` or `hh:mm:ss.ttt` times
//!   with cue settings after them ignored. Inline timestamps
//!   (`hello <00:00:02.600>world`, as both editors' VTT export writes word
//!   timings) become timed words; a stretch with several words between
//!   timestamps shares its time by word length.
//!
//! Formatting tags (`<i>`, `<b>`, `<c.class>`, `<v Speaker>`, `<font ...>`)
//! and SSA codes such as `{\an8}` are taken out; the common character
//! references (`&amp;`, `&lt;`, `&gt;`, `&nbsp;`, `&quot;`, `&#39;`) are
//! read. Cues without text or with an end not after the start are skipped
//! and counted. Checked against `fixtures/subtitles/cases.json`.

use serde::{Deserialize, Serialize};

/// A timed word, seconds from the start of the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A cue: its text (lines joined by `\n`) and times in seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// Timed words, when the file gives word timings (WebVTT timestamps).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

/// What a file held.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parsed {
    pub cues: Vec<Cue>,
    /// Blocks that looked like cues but could not be read.
    pub skipped: usize,
}

/// Which format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Srt,
    Vtt,
}

impl Format {
    /// From a file name's extension; `None` when it is neither.
    pub fn from_name(name: &str) -> Option<Format> {
        let extension = name.rsplit('.').next()?.to_ascii_lowercase();
        match extension.as_str() {
            "srt" => Some(Format::Srt),
            "vtt" => Some(Format::Vtt),
            _ => None,
        }
    }

    /// From the text itself: WebVTT starts with `WEBVTT`.
    pub fn sniff(input: &str) -> Format {
        if input.trim_start_matches('\u{feff}').trim_start().starts_with("WEBVTT") { Format::Vtt } else { Format::Srt }
    }
}

/// Parses `input` as `format` (sniffed from the text when `None`).
pub fn parse(input: &str, format: Option<Format>) -> Parsed {
    let format = format.unwrap_or_else(|| Format::sniff(input));
    let text = input.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut parsed = Parsed::default();
    for (index, block) in text.split("\n\n").enumerate() {
        let lines: Vec<&str> = block.lines().map(str::trim_end).filter(|line| !line.trim().is_empty()).collect();
        if lines.is_empty() {
            continue;
        }
        if format == Format::Vtt {
            let first = lines[0].trim();
            if index == 0 && first.starts_with("WEBVTT") {
                continue;
            }
            if first.starts_with("NOTE") || first == "STYLE" || first.starts_with("STYLE ") || first == "REGION" || first.starts_with("REGION ") {
                continue;
            }
        }
        let Some(timing_index) = lines.iter().position(|line| line.contains("-->")) else {
            parsed.skipped += 1;
            continue;
        };
        if timing_index > 1 {
            parsed.skipped += 1;
            continue;
        }
        let Some((start, end)) = read_timing(lines[timing_index]) else {
            parsed.skipped += 1;
            continue;
        };
        let raw: Vec<&str> = lines[timing_index + 1..].iter().map(|line| line.trim()).collect();
        if end <= start {
            parsed.skipped += 1;
            continue;
        }
        let words = if format == Format::Vtt { timed_words(&raw.join("\n"), start, end) } else { Vec::new() };
        let text = raw.iter().map(|line| clean(line)).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n");
        if text.is_empty() {
            parsed.skipped += 1;
            continue;
        }
        parsed.cues.push(Cue { text, start, end, words });
    }
    parsed
}

/// `start --> end [settings]` in seconds.
fn read_timing(line: &str) -> Option<(f64, f64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((read_time(start.trim())?, read_time(end)?))
}

/// `hh:mm:ss.ttt`, `mm:ss.ttt` (a comma for the dot too) in seconds.
pub fn read_time(text: &str) -> Option<f64> {
    let text = text.trim().replace(',', ".");
    let (clock, fraction) = match text.split_once('.') {
        Some((clock, fraction)) => (clock.to_owned(), fraction.to_owned()),
        None => (text.clone(), String::new()),
    };
    if !fraction.chars().all(|c| c.is_ascii_digit()) || fraction.len() > 3 {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 || parts.iter().any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let numbers: Vec<f64> = parts.iter().map(|part| part.parse::<f64>().unwrap_or(0.0)).collect();
    let (hours, minutes, seconds) = if numbers.len() == 3 { (numbers[0], numbers[1], numbers[2]) } else { (0.0, numbers[0], numbers[1]) };
    if minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }
    let millis = if fraction.is_empty() { 0.0 } else { format!("{fraction:0<3}").parse::<f64>().ok()? };
    Some(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
}

/// Text without tags, SSA codes and character references, spaces tidied.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                }
            }
            '{' if chars.peek() == Some(&'\\') => {
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lrm;", "")
        .replace("&rlm;", "")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// WebVTT inline timestamps as timed words; none when the cue has none.
fn timed_words(text: &str, cue_start: f64, cue_end: f64) -> Vec<Word> {
    // Pieces of text, each with the time it starts.
    let mut pieces: Vec<(f64, String)> = vec![(cue_start, String::new())];
    let mut stamps = 0;
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        pieces.last_mut().unwrap().1.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            pieces.last_mut().unwrap().1.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        match read_time(tag) {
            Some(time) => {
                stamps += 1;
                pieces.push((time.clamp(cue_start, cue_end), String::new()));
            }
            // Formatting tags stay in for `clean` to take out.
            None => pieces.last_mut().unwrap().1.push_str(&rest[open..open + close + 1]),
        }
        rest = &rest[open + close + 1..];
    }
    pieces.last_mut().unwrap().1.push_str(rest);
    if stamps == 0 {
        return Vec::new();
    }
    let mut words = Vec::new();
    for (index, (start, piece)) in pieces.iter().enumerate() {
        let end = pieces.get(index + 1).map_or(cue_end, |next| next.0).max(*start);
        let tokens: Vec<String> = clean(piece).split_whitespace().map(str::to_owned).collect();
        if tokens.is_empty() {
            continue;
        }
        let weights: Vec<f64> = tokens.iter().map(|token| token.chars().count() as f64 + 1.0).collect();
        let total: f64 = weights.iter().sum();
        let mut cursor = *start;
        for (token, weight) in tokens.into_iter().zip(weights) {
            let next = cursor + (end - start) * weight / total;
            words.push(Word { text: token, start: round_ms(cursor), end: round_ms(next) });
            cursor = next;
        }
    }
    words
}

fn round_ms(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn parses_like_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/subtitles/cases.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let format = match case["format"].as_str() {
                Some("srt") => Some(Format::Srt),
                Some("vtt") => Some(Format::Vtt),
                _ => None,
            };
            let got = parse(case["input"].as_str().unwrap(), format);
            let want: Parsed = serde_json::from_value(case["parsed"].clone()).unwrap();
            assert_eq!(got.skipped, want.skipped, "{name}: skipped");
            assert_eq!(got.cues.len(), want.cues.len(), "{name}: {:?}", got.cues);
            for (a, b) in got.cues.iter().zip(&want.cues) {
                assert_eq!(a.text, b.text, "{name}");
                assert!((a.start - b.start).abs() < 1e-9 && (a.end - b.end).abs() < 1e-9, "{name}: {a:?} vs {b:?}");
                assert_eq!(a.words.len(), b.words.len(), "{name}: {:?}", a.words);
                for (x, y) in a.words.iter().zip(&b.words) {
                    assert_eq!(x.text, y.text, "{name}");
                    assert!((x.start - y.start).abs() < 1e-9 && (x.end - y.end).abs() < 1e-9, "{name}: {x:?} vs {y:?}");
                }
            }
        }
    }

    #[test]
    fn formats_from_names_and_text() {
        assert_eq!(Format::from_name("a.SRT"), Some(Format::Srt));
        assert_eq!(Format::from_name("b.vtt"), Some(Format::Vtt));
        assert_eq!(Format::from_name("c.ass"), None);
        assert_eq!(Format::sniff("\u{feff}WEBVTT\n\n"), Format::Vtt);
        assert_eq!(read_time("01:02:03,5"), Some(3723.5));
        assert_eq!(read_time("61:00.000"), None);
    }
}

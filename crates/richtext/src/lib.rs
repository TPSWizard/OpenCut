//! Rich text: colour, weight, slant and size for parts of one text clip.
//!
//! A text element keeps its styled ranges in the `richText` param, as JSON
//! with the text they were made for: `{"text": "...", "spans": [{"start":
//! 0, "end": 5, "color": "#ff0000", "bold": true, "italic": false,
//! "scale": 1.5}]}`. Positions count characters (Unicode scalar values),
//! end exclusive; a field left out keeps the clip's own style. When the
//! clip's text has been edited since (anywhere: the inspector, the canvas,
//! the API), [`rebase`] moves the styles with it: the unchanged start and
//! end keep theirs, and text typed inside a styled run takes its style.
//! Both editors use this crate (the web through WASM); checked against
//! `fixtures/rich-text/cases.json`.

use serde::{Deserialize, Serialize};

/// The smallest and largest size, relative to the clip's font size.
pub const MIN_SCALE: f64 = 0.25;
pub const MAX_SCALE: f64 = 4.0;

/// One styled range.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

/// The styled ranges and the text they were made for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

/// One character's style; `None` keeps the clip's own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

impl CharStyle {
    pub fn is_plain(&self) -> bool {
        self.color.is_none() && self.bold.is_none() && self.italic.is_none() && self.scale.is_none()
    }

    /// The size relative to the clip's font size (1 when not set).
    pub fn size(&self) -> f64 {
        self.scale.unwrap_or(1.0)
    }
}

/// What to set on a range; fields left out are not changed.
pub type Patch = CharStyle;

/// `#rgb` or `#rrggbb`, as `#rrggbb` in lower case.
pub fn normalize_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => Some(format!("#{}", hex.chars().flat_map(|c| [c, c]).collect::<String>().to_ascii_lowercase())),
        6 => Some(format!("#{}", hex.to_ascii_lowercase())),
        _ => None,
    }
}

fn clean_scale(scale: f64) -> Option<f64> {
    scale.is_finite().then(|| scale.clamp(MIN_SCALE, MAX_SCALE))
}

fn clean(style: CharStyle) -> CharStyle {
    CharStyle {
        color: style.color.as_deref().and_then(normalize_color),
        bold: style.bold,
        italic: style.italic,
        scale: style.scale.and_then(clean_scale),
    }
}

/// The stored JSON, or `None` when it is not valid. Bad colours and sizes
/// inside are dropped; empty ranges are kept out.
pub fn parse(json: &str) -> Option<RichText> {
    let mut rich: RichText = serde_json::from_str(json).ok()?;
    rich.spans.retain(|span| span.end > span.start);
    for span in &mut rich.spans {
        let style = clean(CharStyle { color: span.color.take(), bold: span.bold, italic: span.italic, scale: span.scale });
        span.color = style.color;
        span.scale = style.scale;
    }
    Some(rich)
}

pub fn encode(rich: &RichText) -> String {
    serde_json::to_string(rich).expect("rich text serialises")
}

/// Each character's style in `text` from `spans` (later spans win, field
/// by field). Ranges past the end are cut off.
pub fn char_styles(text: &str, spans: &[Span]) -> Vec<CharStyle> {
    let count = text.chars().count();
    let mut styles = vec![CharStyle::default(); count];
    for span in spans {
        for style in styles.iter_mut().take(span.end.min(count)).skip(span.start) {
            overlay(style, &CharStyle { color: span.color.clone(), bold: span.bold, italic: span.italic, scale: span.scale });
        }
    }
    styles
}

fn overlay(style: &mut CharStyle, patch: &CharStyle) {
    if patch.color.is_some() {
        style.color = patch.color.clone();
    }
    if patch.bold.is_some() {
        style.bold = patch.bold;
    }
    if patch.italic.is_some() {
        style.italic = patch.italic;
    }
    if patch.scale.is_some() {
        style.scale = patch.scale;
    }
}

/// The fewest spans that give `styles`: runs of equal, non-plain styles.
pub fn spans_from_styles(styles: &[CharStyle]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut index = 0;
    while index < styles.len() {
        let style = &styles[index];
        let mut end = index + 1;
        while end < styles.len() && styles[end] == *style {
            end += 1;
        }
        if !style.is_plain() {
            spans.push(Span {
                start: index,
                end,
                color: style.color.clone(),
                bold: style.bold,
                italic: style.italic,
                scale: style.scale,
            });
        }
        index = end;
    }
    spans
}

/// Each character's style for `text`, carried over from the text the
/// ranges were made for: the unchanged start and end keep their styles;
/// text that replaced the middle takes the style around it when both sides
/// match (typed inside a styled run), and none otherwise.
pub fn rebase(rich: &RichText, text: &str) -> Vec<CharStyle> {
    let old: Vec<char> = rich.text.chars().collect();
    let new: Vec<char> = text.chars().collect();
    let old_styles = char_styles(&rich.text, &rich.spans);
    if old == new {
        return old_styles;
    }
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old.iter().rev().zip(new.iter().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
    let inserted = new.len() - prefix - suffix;
    let before = prefix.checked_sub(1).map(|index| &old_styles[index]);
    let after = old_styles.get(old.len() - suffix);
    let middle = match (before, after) {
        (Some(a), Some(b)) if a == b => a.clone(),
        _ => CharStyle::default(),
    };
    let mut styles = Vec::with_capacity(new.len());
    styles.extend_from_slice(&old_styles[..prefix]);
    styles.extend(std::iter::repeat_n(middle, inserted));
    styles.extend_from_slice(&old_styles[old.len() - suffix..]);
    styles
}

/// The rich text for `text` with `patch` set on characters `start..end`
/// (clamped), starting from what was stored (if anything).
pub fn style_range(stored: Option<&RichText>, text: &str, start: usize, end: usize, patch: &Patch) -> RichText {
    let mut styles = stored.map_or_else(|| vec![CharStyle::default(); text.chars().count()], |rich| rebase(rich, text));
    let patch = clean(patch.clone());
    let end = end.min(styles.len());
    for style in styles.iter_mut().take(end).skip(start.min(end)) {
        overlay(style, &patch);
    }
    RichText { text: text.to_owned(), spans: spans_from_styles(&styles) }
}

/// The rich text for `text` with characters `start..end` back to the
/// clip's own style.
pub fn clear_range(stored: Option<&RichText>, text: &str, start: usize, end: usize) -> RichText {
    let mut styles = stored.map_or_else(|| vec![CharStyle::default(); text.chars().count()], |rich| rebase(rich, text));
    let end = end.min(styles.len());
    for style in styles.iter_mut().take(end).skip(start.min(end)) {
        *style = CharStyle::default();
    }
    RichText { text: text.to_owned(), spans: spans_from_styles(&styles) }
}

/// Each character's style for the clip's current `text`, from the stored
/// JSON (`None` when there is nothing to style).
pub fn styles_for(json: &str, text: &str) -> Option<Vec<CharStyle>> {
    let rich = parse(json)?;
    let styles = rebase(&rich, text);
    styles.iter().any(|style| !style.is_plain()).then_some(styles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rich-text/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn styles(value: &Value) -> Vec<CharStyle> {
        serde_json::from_value(value.clone()).expect("styles")
    }

    fn rich(value: &Value) -> Option<RichText> {
        (!value.is_null()).then(|| serde_json::from_value(value.clone()).expect("rich"))
    }

    #[test]
    fn char_styles_match_the_reference() {
        for case in cases()["charStyles"].as_array().unwrap() {
            let parsed = parse(case["json"].as_str().unwrap());
            if case["valid"].as_bool() == Some(false) {
                assert_eq!(parsed, None, "{case}");
                continue;
            }
            let got = rebase(&parsed.expect("valid"), case["text"].as_str().unwrap());
            assert_eq!(got, styles(&case["styles"]), "{}", case["name"]);
        }
    }

    #[test]
    fn rebasing_after_edits_matches_the_reference() {
        for case in cases()["rebase"].as_array().unwrap() {
            let stored = rich(&case["stored"]).unwrap();
            let got = rebase(&stored, case["text"].as_str().unwrap());
            assert_eq!(got, styles(&case["styles"]), "{}", case["name"]);
        }
    }

    #[test]
    fn styling_and_clearing_ranges_match_the_reference() {
        for case in cases()["edits"].as_array().unwrap() {
            let stored = rich(&case["stored"]);
            let text = case["text"].as_str().unwrap();
            let (start, end) = (case["start"].as_u64().unwrap() as usize, case["end"].as_u64().unwrap() as usize);
            let got = match case["patch"].is_null() {
                true => clear_range(stored.as_ref(), text, start, end),
                false => style_range(stored.as_ref(), text, start, end, &serde_json::from_value(case["patch"].clone()).unwrap()),
            };
            assert_eq!(got, rich(&case["result"]).unwrap(), "{}", case["name"]);
            assert_eq!(parse(&encode(&got)), Some(got.clone()), "round trip");
        }
    }

    #[test]
    fn colours_are_normalised() {
        assert_eq!(normalize_color("#F0a"), Some("#ff00aa".into()));
        assert_eq!(normalize_color("#12AB34"), Some("#12ab34".into()));
        assert_eq!(normalize_color("red"), None);
        assert_eq!(normalize_color("#12345"), None);
    }
}

//! Captions from word timings: the same chunking as the web app's
//! `buildCaptionChunksFromWords` (a chunk closes after N words, at sentence
//! punctuation, or at a pause longer than a second, and short chunks stay
//! up for a readable minimum without running into the next), and the text
//! elements the web app's `buildSubtitleTextElement` makes from them.

use time::MediaTime;

use crate::document::{CanvasSize, CaptionWord, Element};

pub const DEFAULT_WORDS_PER_CAPTION: usize = 3;
pub const MIN_CAPTION_DURATION_SECONDS: f64 = 0.8;
/// A pause between words long enough to start a new caption.
pub const MAX_CAPTION_WORD_GAP_SECONDS: f64 = 1.0;
/// The web app's subtitle style: font size as a ratio of a 90-unit
/// reference, bold, white, centred, 5 % above the bottom.
pub const SUBTITLE_FONT_SIZE: f64 = 5.0;
const SUBTITLE_BOTTOM_MARGIN_RATIO: f64 = 0.05;
const FONT_SIZE_SCALE_REFERENCE: f64 = 90.0;
/// Highlight the generated captions get, as the web app's generated ones do.
pub const GENERATED_CAPTION_HIGHLIGHT: &str = "word";

/// A spoken word with absolute times in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A caption: its text, when it shows (seconds) and the words it carries.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionChunk {
    pub text: String,
    pub start: f64,
    pub duration: f64,
    pub words: Vec<TimedWord>,
}

fn ends_sentence(text: &str) -> bool {
    let trimmed = text.trim_end_matches(|c| matches!(c, '"' | '\'' | ')' | ']'));
    trimmed.ends_with(['.', '!', '?', '。', '！', '？'])
}

pub fn build_caption_chunks(
    words: &[TimedWord],
    words_per_chunk: usize,
    min_duration: f64,
    max_gap: f64,
) -> Vec<CaptionChunk> {
    let per_chunk = words_per_chunk.max(1);
    let mut groups: Vec<Vec<TimedWord>> = Vec::new();
    let mut current: Vec<TimedWord> = Vec::new();
    for raw in words {
        let text = raw.text.trim();
        if text.is_empty() {
            continue;
        }
        let word = TimedWord { text: text.to_owned(), start: raw.start, end: raw.end.max(raw.start) };
        if let Some(previous) = current.last() {
            if word.start - previous.end > max_gap {
                groups.push(std::mem::take(&mut current));
            }
        }
        let closes = ends_sentence(&word.text);
        current.push(word);
        if current.len() >= per_chunk || closes {
            groups.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let mut chunks = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        let start = group[0].start;
        let spoken_end = group[group.len() - 1].end;
        let mut end = spoken_end.max(start + min_duration);
        if let Some(next) = groups.get(index + 1) {
            end = end.min(next[0].start);
        }
        let duration = (end - start).max(spoken_end - start).max(0.01);
        chunks.push(CaptionChunk {
            text: group.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" "),
            start,
            duration,
            words: group.clone(),
        });
    }
    chunks
}

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value.max(0.0)).unwrap_or(MediaTime::ZERO)
}

/// A caption as a text element the way the web app builds one: the
/// subtitle style, the words relative to the caption's start, and the
/// generated highlight. The web app measures the text to place it; here
/// it sits a line above the bottom margin.
pub fn caption_element(index: usize, chunk: &CaptionChunk, canvas: CanvasSize) -> Element {
    let mut element = Element::text(&chunk.text, seconds(chunk.start));
    element.name = format!("Caption {}", index + 1);
    element.duration = seconds(chunk.duration).max(MediaTime::ONE_TICK);
    let font_px = SUBTITLE_FONT_SIZE * (canvas.height as f64 / FONT_SIZE_SCALE_REFERENCE);
    let margin = canvas.height as f64 * SUBTITLE_BOTTOM_MARGIN_RATIO;
    // Position is an offset from the canvas centre; the text's own height is
    // about one line (1.2 × font size), placed above the margin.
    let position_y = canvas.height as f64 / 2.0 - margin - font_px * 1.2 / 2.0;
    element
        .params
        .set("fontSize", SUBTITLE_FONT_SIZE)
        .set("fontFamily", "Arial")
        .set("color", "#ffffff")
        .set("textAlign", "center")
        .set("fontWeight", "bold")
        .set("transform.positionX", 0.0)
        .set("transform.positionY", position_y)
        .set("caption.highlight", GENERATED_CAPTION_HIGHLIGHT)
        .set("caption.activeColor", "#ffd60a")
        .set("caption.boxColor", "#e5484d");
    element.words = chunk
        .words
        .iter()
        .filter(|word| !word.text.trim().is_empty())
        .map(|word| {
            let start = (word.start - chunk.start).clamp(0.0, chunk.duration);
            let end = (word.end - chunk.start).clamp(start, chunk.duration);
            CaptionWord { text: word.text.trim().to_owned(), start: seconds(start), end: seconds(end) }
        })
        .collect();
    element
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord { text: text.to_owned(), start, end }
    }

    #[test]
    fn chunks_close_on_count_punctuation_and_pauses() {
        let words = vec![
            word("The", 0.0, 0.2),
            word("quick", 0.2, 0.5),
            word("brown", 0.5, 0.8),
            word("fox.", 0.8, 1.1),
            word("Jumps", 1.2, 1.5),
            word("over", 3.0, 3.3),
            word("it", 3.3, 3.5),
        ];
        let chunks = build_caption_chunks(&words, 3, 0.8, 1.0);
        assert_eq!(chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["The quick brown", "fox.", "Jumps", "over it"]);
        assert_eq!(chunks[0].start, 0.0);
        assert!((chunks[0].duration - 0.8).abs() < 1e-9, "held to the minimum, which ends at the next chunk");
        assert!((chunks[1].duration - 0.4).abs() < 1e-9, "capped by the next chunk at 1.2");
        assert!((chunks[2].duration - 0.8).abs() < 1e-9, "a pause follows, so the minimum holds");
        assert!((chunks[3].duration - 0.8).abs() < 1e-9);
    }

    #[test]
    fn caption_elements_carry_relative_words_and_the_style() {
        let chunk = CaptionChunk {
            text: "fox jumps over".to_owned(),
            start: 2.0,
            duration: 1.0,
            words: vec![word("fox", 2.0, 2.3), word("jumps", 2.3, 2.6), word("over", 2.6, 3.4)],
        };
        let element = caption_element(1, &chunk, CanvasSize { width: 1920, height: 1080 });
        assert_eq!(element.name, "Caption 2");
        assert_eq!(element.start_time.to_seconds_f64(), 2.0);
        assert_eq!(element.duration.to_seconds_f64(), 1.0);
        assert_eq!(element.params.text("content", ""), "fox jumps over");
        assert_eq!(element.params.text("caption.highlight", ""), "word");
        assert_eq!(element.params.text("fontWeight", ""), "bold");
        assert_eq!(element.words.len(), 3);
        assert_eq!(element.words[1].start.to_seconds_f64(), 0.3);
        assert_eq!(element.words[2].end.to_seconds_f64(), 1.0, "clamped into the caption");
        assert!(element.params.number("transform.positionY", 0.0) > 400.0, "near the bottom");
    }
}

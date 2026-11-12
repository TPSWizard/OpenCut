//! Filler words ("um", "uh", "ähm", "euh", …) among timed words, and the
//! stretches to cut them out softly: each filler's span with a little
//! padding, never reaching into the neighbouring words, merged where
//! fillers follow each other. The lists and the arithmetic are the web
//! app's `src/fillers/detect.ts`.

use std::collections::HashSet;

use crate::captions::TimedWord;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillerOptions {
    /// Seconds taken on each side of a filler, held back from the neighbouring words.
    pub padding_seconds: f64,
    /// A "filler" longer than this is a word the list happened to match.
    pub max_seconds: f64,
}

impl Default for FillerOptions {
    fn default() -> Self {
        Self { padding_seconds: 0.04, max_seconds: 1.5 }
    }
}

/// Normalised fillers (lower case, letter runs collapsed) per language.
pub const FILLER_WORDS: &[(&str, &[&str])] = &[
    ("en", &["um", "uh", "er", "erm", "ah", "hm", "mhm", "mm", "uhm", "ehm"]),
    ("de", &["äh", "ähm", "hm", "öhm", "mh", "ehm"]),
    ("fr", &["euh", "heu", "hum", "hm", "ben", "bah"]),
    ("es", &["eh", "em", "este", "mm", "hm", "ehm"]),
    ("pt", &["hum", "hã", "ahn", "ã", "hm", "eh"]),
    ("it", &["ehm", "mh", "hm", "eh", "uhm"]),
];

/// Fillers that are a real word somewhere, kept out of the automatic set.
const AUTO_EXCLUDED: &[&str] = &["ben", "bah", "este", "em"];

/// `(code, label)` for the language choice.
pub const FILLER_LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Any"),
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("it", "Italian"),
];

/// The filler set for a language code (`auto` or an unknown code takes the safe union).
pub fn filler_set(language: &str) -> HashSet<&'static str> {
    let code = language.to_lowercase();
    let code = code.split('-').next().unwrap_or("");
    if let Some((_, list)) = FILLER_WORDS.iter().find(|(lang, _)| *lang == code) {
        return list.iter().copied().collect();
    }
    FILLER_WORDS
        .iter()
        .flat_map(|(_, list)| list.iter().copied())
        .filter(|word| !AUTO_EXCLUDED.contains(word))
        .collect()
}

/// Lower case, letters only, runs of a letter collapsed: "Ummm," → "um".
pub fn normalize_word(text: &str) -> String {
    let mut out = String::new();
    let mut last: Option<char> = None;
    for c in text.chars().flat_map(|c| c.to_lowercase()) {
        if !c.is_alphabetic() {
            continue;
        }
        if last != Some(c) {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

/// A filler among the words: the word and the normalised form that matched.
#[derive(Clone, Debug, PartialEq)]
pub struct FillerHit {
    pub word: TimedWord,
    pub filler: String,
}

pub fn find_fillers(words: &[TimedWord], language: &str, options: &FillerOptions) -> Vec<FillerHit> {
    let set = filler_set(language);
    words
        .iter()
        .filter(|word| word.end - word.start <= options.max_seconds)
        .filter_map(|word| {
            let filler = normalize_word(&word.text);
            (!filler.is_empty() && set.contains(filler.as_str())).then(|| FillerHit { word: word.clone(), filler })
        })
        .collect()
}

/// The stretches to cut `(start, end)`: each filler padded, clamped to the
/// words either side, merged where they touch.
pub fn filler_intervals(words: &[TimedWord], fillers: &[FillerHit], options: &FillerOptions) -> Vec<(f64, f64)> {
    let mut sorted: Vec<&TimedWord> = words.iter().collect();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    let is_filler = |word: &TimedWord| fillers.iter().any(|hit| hit.word.start == word.start && hit.word.end == word.end);
    let mut raw: Vec<(f64, f64)> = Vec::new();
    for (index, word) in sorted.iter().enumerate() {
        if !is_filler(word) {
            continue;
        }
        let start = (word.start - options.padding_seconds).max(index.checked_sub(1).map(|i| sorted[i].end).unwrap_or(0.0));
        let end = (word.end + options.padding_seconds).min(sorted.get(index + 1).map(|next| next.start).unwrap_or(f64::INFINITY));
        if end > start {
            raw.push((start, end));
        }
    }
    raw.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in raw {
        match merged.last_mut() {
            Some(last) if start <= last.1 + 0.02 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// A transcription prompt that keeps Whisper from dropping hesitations.
pub const FILLER_PROMPT: &str = "Um, uh, so, like, hmm, erm... okay, you know, I mean, uh, let me think.";

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord { text: text.to_owned(), start, end }
    }

    fn words() -> Vec<TimedWord> {
        vec![
            word("So,", 0.0, 0.3),
            word("um,", 0.4, 0.7),
            word("the", 0.75, 0.9),
            word("plan", 0.9, 1.3),
            word("is", 1.3, 1.45),
            word("uhhh", 1.5, 1.9),
            word("Umm", 1.95, 2.2),
            word("simple.", 2.3, 2.8),
            word("Hmm", 4.0, 6.0),
        ]
    }

    #[test]
    fn normalises_and_knows_the_languages() {
        assert_eq!(normalize_word("Ummm,"), "um");
        assert_eq!(normalize_word("Ähm…"), "ähm");
        assert_eq!(normalize_word("123"), "");
        assert!(filler_set("en").contains("um"));
        assert!(filler_set("fr").contains("ben"));
        assert!(!filler_set("auto").contains("ben"));
        assert!(filler_set("auto").contains("ähm"));
        assert!(filler_set("xx-YY").contains("um"));
    }

    #[test]
    fn pads_fillers_without_touching_neighbours_and_merges_runs() {
        let words = words();
        let options = FillerOptions::default();
        let fillers = find_fillers(&words, "en", &options);
        assert_eq!(fillers.iter().map(|hit| hit.filler.as_str()).collect::<Vec<_>>(), vec!["um", "uh", "um"]);
        let intervals = filler_intervals(&words, &fillers, &options);
        assert_eq!(intervals.len(), 2);
        assert!((intervals[0].0 - 0.36).abs() < 1e-9 && (intervals[0].1 - 0.74).abs() < 1e-9, "{intervals:?}");
        assert!((intervals[1].0 - 1.46).abs() < 1e-9 && (intervals[1].1 - 2.24).abs() < 1e-9, "{intervals:?}");
        assert!(find_fillers(&[word("The", 0.0, 0.2)], "auto", &options).is_empty());
    }
}

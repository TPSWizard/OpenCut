//! Script to B-roll: a script becomes a shot list, one shot per line (the
//! voice-over's lines, see [`crate::voiceover`]), each timed for reading at
//! a steady pace and given a stock search query. The editors then search
//! stock media for each query and lay the results out in order. The web
//! app's `src/broll/plan.ts` is the same arithmetic, checked against
//! `fixtures/broll/cases.json`.
//!
//! The query here is picked without a model: the line's words minus common
//! words, the longest three kept in their order. The API worker can ask
//! Claude for better queries for the same lines (`POST /broll/plan`); the
//! timings never change.

use crate::voiceover::{MAX_LINE_CHARS, split_script};

/// Reading pace for timing shots, words a second (about 150 a minute).
pub const WORDS_PER_SECOND: f64 = 2.5;
/// No shot is shorter than this, seconds.
pub const MIN_SHOT_SECONDS: f64 = 1.5;
/// Longest query kept, characters.
pub const MAX_QUERY_CHARS: usize = 60;
/// Words a query keeps.
pub const QUERY_WORDS: usize = 3;

/// One shot: the script line it covers, what to search for, and when it
/// plays (seconds from the start of the B-roll).
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub text: String,
    pub query: String,
    pub start: f64,
    pub duration: f64,
}

/// Common English words left out of queries.
const STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "all", "also", "am", "an", "and", "any", "are", "around", "as", "at",
    "back", "be", "because", "been", "before", "being", "below", "between", "both", "but", "by", "can", "could",
    "did", "do", "does", "doing", "done", "down", "during", "each", "even", "every", "few", "first", "for", "from",
    "get", "gets", "getting", "go", "goes", "going", "got", "had", "has", "have", "having", "he", "her", "here",
    "hers", "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just", "know", "let", "like", "made",
    "make", "makes", "many", "me", "might", "more", "most", "much", "must", "my", "need", "never", "new", "next",
    "no", "not", "now", "of", "off", "on", "once", "one", "only", "or", "other", "our", "out", "over", "own",
    "really", "right", "same", "see", "she", "should", "so", "some", "still", "such", "take", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "thing", "things", "think", "this", "those",
    "through", "to", "today", "too", "two", "under", "until", "up", "us", "use", "very", "want", "was", "way",
    "we", "well", "were", "what", "when", "where", "which", "while", "who", "why", "will", "with", "would", "you",
    "your", "yours",
];

/// Words of a line for a query: lower case, letters, digits and inner
/// apostrophes or hyphens, possessive `'s` dropped.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’' || c == '-'))
        .map(|word| word.trim_matches(|c: char| c == '\'' || c == '’' || c == '-').to_lowercase())
        .map(|word| word.strip_suffix("'s").or_else(|| word.strip_suffix("’s")).map(str::to_owned).unwrap_or(word))
        .filter(|word| !word.is_empty())
        .collect()
}

/// The search query for a line: its longest uncommon words (three, at
/// least three letters, first appearance kept), in the order they appear.
/// A line of only common words, or in a script without spaces, is used
/// as it is, shortened.
pub fn query_for(text: &str) -> String {
    let mut seen = Vec::<String>::new();
    for word in words(text) {
        if word.chars().count() >= 3 && !STOP_WORDS.contains(&word.as_str()) && !seen.contains(&word) {
            seen.push(word);
        }
    }
    let mut ranked: Vec<(usize, &String)> = seen.iter().enumerate().collect();
    // Longest first; the earlier word wins a tie.
    ranked.sort_by(|a, b| b.1.chars().count().cmp(&a.1.chars().count()).then(a.0.cmp(&b.0)));
    let mut keep: Vec<(usize, &String)> = ranked.into_iter().take(QUERY_WORDS).collect();
    keep.sort_by_key(|(index, _)| *index);
    let query = if keep.is_empty() {
        text.trim().to_owned()
    } else {
        keep.iter().map(|(_, word)| word.as_str()).collect::<Vec<_>>().join(" ")
    };
    query.chars().take(MAX_QUERY_CHARS).collect::<String>().trim().to_owned()
}

/// How long a line plays at `words_per_second`.
pub fn shot_seconds(text: &str, words_per_second: f64) -> f64 {
    let pace = if words_per_second.is_finite() && words_per_second > 0.0 { words_per_second } else { WORDS_PER_SECOND };
    let count = text.split_whitespace().count().max(1) as f64;
    // Scripts without spaces (Chinese, Japanese): about three characters a word.
    let count = if text.split_whitespace().count() <= 1 { count.max(text.chars().count() as f64 / 3.0) } else { count };
    round_ms((count / pace).max(MIN_SHOT_SECONDS))
}

fn round_ms(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// The shot list for `script`: one shot per line, back to back with the
/// voice-over's pauses between them.
pub fn plan_shots(script: &str, words_per_second: f64) -> Vec<Shot> {
    let mut start = 0.0;
    split_script(script, MAX_LINE_CHARS)
        .into_iter()
        .map(|line| {
            let duration = shot_seconds(&line.text, words_per_second);
            let shot = Shot { query: query_for(&line.text), text: line.text, start: round_ms(start), duration };
            start += duration + line.pause_after;
            shot
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        serde_json::from_str(include_str!("../../../fixtures/broll/cases.json")).unwrap()
    }

    #[test]
    fn queries_match_the_shared_cases() {
        for case in cases()["queries"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            assert_eq!(query_for(text), case["query"].as_str().unwrap(), "{text}");
        }
    }

    #[test]
    fn shot_lists_match_the_shared_cases() {
        for case in cases()["plans"].as_array().unwrap() {
            let script = case["script"].as_str().unwrap();
            let pace = case["wordsPerSecond"].as_f64().unwrap_or(WORDS_PER_SECOND);
            let got = plan_shots(script, pace);
            let want = case["shots"].as_array().unwrap();
            assert_eq!(got.len(), want.len(), "{script}");
            for (shot, want) in got.iter().zip(want) {
                assert_eq!(shot.text, want["text"].as_str().unwrap());
                assert_eq!(shot.query, want["query"].as_str().unwrap());
                assert_eq!(shot.start, want["start"].as_f64().unwrap(), "{}", shot.text);
                assert_eq!(shot.duration, want["duration"].as_f64().unwrap(), "{}", shot.text);
            }
        }
    }
}

//! Caption editing in bulk (row 94): find and replace across every
//! caption, highlight what a search finds, and auto emoji. The same for
//! both editors (the web through WASM).
//!
//! A caption is its timed words (the content's whitespace-separated tokens
//! in order); its content is the words joined by spaces, so every change
//! here rewrites both. A search is whole words in order, ignoring case and
//! the punctuation around each word: "the plan" finds "The plan," but not
//! "planned". A replacement keeps the punctuation before the first and
//! after the last matched word, and shares the matched words' time among
//! its own words (by length), so captions stay in time; an empty
//! replacement takes the words out. Matched keywords stay keywords.
//!
//! Auto emoji adds one emoji (from a small word list of our own) after the
//! first word in each caption that has one, as a word of its own spoken at
//! that word's end; captions that already have one are left alone.
//! Checked against `fixtures/captionedit/cases.json`.

use serde::{Deserialize, Serialize};

/// A timed word: times are ticks from the caption's start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: i64,
    pub end: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keyword: bool,
}

/// A caption: its id and words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub id: String,
    pub words: Vec<Word>,
}

/// Where a search matched: a caption and its words `[start, start + count)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub caption: usize,
    pub start: usize,
    pub count: usize,
}

/// A caption after an edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edited {
    pub id: String,
    pub content: String,
    pub words: Vec<Word>,
}

/// A word without the punctuation around it, in lower case.
pub fn core(text: &str) -> String {
    text.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// A word's punctuation before and after its core.
fn edges(text: &str) -> (&str, &str) {
    let start = text.find(|c: char| c.is_alphanumeric()).unwrap_or(text.len());
    let end = text.rfind(|c: char| c.is_alphanumeric()).map_or(start, |at| at + text[at..].chars().next().map_or(1, char::len_utf8));
    (&text[..start], &text[end.max(start)..])
}

/// The search's words (cores); none for a search of only punctuation.
fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(core).filter(|term| !term.is_empty()).collect()
}

/// Every place the search matches, captions in order, not overlapping.
pub fn find(captions: &[Caption], query: &str) -> Vec<Match> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for (caption_index, caption) in captions.iter().enumerate() {
        let cores: Vec<String> = caption.words.iter().map(|word| core(&word.text)).collect();
        let mut at = 0;
        while at + terms.len() <= cores.len() {
            if cores[at..at + terms.len()] == terms[..] {
                found.push(Match { caption: caption_index, start: at, count: terms.len() });
                at += terms.len();
            } else {
                at += 1;
            }
        }
    }
    found
}

fn content(words: &[Word]) -> String {
    words.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" ")
}

/// Every match replaced by `replacement`; only the captions that changed
/// come back.
pub fn replace(captions: &[Caption], query: &str, replacement: &str) -> Vec<Edited> {
    let matches = find(captions, query);
    let tokens: Vec<&str> = replacement.split_whitespace().collect();
    let mut out = Vec::new();
    for (caption_index, caption) in captions.iter().enumerate() {
        let here: Vec<&Match> = matches.iter().filter(|found| found.caption == caption_index).collect();
        if here.is_empty() {
            continue;
        }
        let mut words = Vec::with_capacity(caption.words.len());
        let mut at = 0;
        for found in here {
            words.extend_from_slice(&caption.words[at..found.start]);
            let span = &caption.words[found.start..found.start + found.count];
            let (before, _) = edges(&span[0].text);
            let (_, after) = edges(&span[span.len() - 1].text);
            let keyword = span.iter().any(|word| word.keyword);
            let (start, end) = (span[0].start, span[span.len() - 1].end);
            if !tokens.is_empty() {
                // Share the time by length (a letter for each space between).
                let weights: Vec<i64> = tokens.iter().map(|token| token.chars().count() as i64 + 1).collect();
                let total: i64 = weights.iter().sum();
                let mut cursor = start;
                let mut used = 0;
                for (index, token) in tokens.iter().enumerate() {
                    used += weights[index];
                    let next = if index + 1 == tokens.len() { end } else { start + (end - start) * used / total };
                    let mut text = String::new();
                    if index == 0 {
                        text.push_str(before);
                    }
                    text.push_str(token);
                    if index + 1 == tokens.len() {
                        text.push_str(after);
                    }
                    words.push(Word { text, start: cursor, end: next, keyword });
                    cursor = next;
                }
            }
            at = found.start + found.count;
        }
        words.extend_from_slice(&caption.words[at..]);
        out.push(Edited { id: caption.id.clone(), content: content(&words), words });
    }
    out
}

/// Every match made a keyword (drawn in the keyword colour and size);
/// only the captions that changed come back.
pub fn highlight(captions: &[Caption], query: &str) -> Vec<Edited> {
    let matches = find(captions, query);
    let mut out = Vec::new();
    for (caption_index, caption) in captions.iter().enumerate() {
        let mut words = caption.words.clone();
        let mut changed = false;
        for found in matches.iter().filter(|found| found.caption == caption_index) {
            for word in &mut words[found.start..found.start + found.count] {
                changed |= !word.keyword;
                word.keyword = true;
            }
        }
        if changed {
            out.push(Edited { id: caption.id.clone(), content: content(&words), words });
        }
    }
    out
}

/// Our own word list: a word (its core; plain plurals match too) and the
/// emoji added after it. Very common words (no, yes, new, time, look) are
/// left out so captions are not covered in emoji.
pub const EMOJI: &[(&str, &str)] = &[
    ("love", "❤️"), ("heart", "❤️"), ("fire", "🔥"), ("hot", "🔥"), ("money", "💰"), ("cash", "💰"), ("rich", "💰"),
    ("laugh", "😂"), ("funny", "😂"), ("lol", "😂"), ("happy", "😊"), ("smile", "😊"), ("sad", "😢"), ("cry", "😢"),
    ("idea", "💡"), ("tip", "💡"), ("rocket", "🚀"), ("launch", "🚀"), ("growth", "📈"), ("grow", "📈"), ("music", "🎵"),
    ("song", "🎵"), ("pizza", "🍕"), ("food", "🍔"), ("coffee", "☕"), ("phone", "📱"), ("car", "🚗"),
    ("home", "🏠"), ("house", "🏠"), ("world", "🌍"), ("earth", "🌍"), ("sun", "☀️"), ("star", "⭐"), ("win", "🏆"),
    ("winner", "🏆"), ("party", "🎉"), ("celebrate", "🎉"), ("birthday", "🎂"), ("gift", "🎁"), ("book", "📚"),
    ("learn", "📚"), ("school", "🏫"), ("work", "💼"), ("job", "💼"), ("camera", "📷"), ("photo", "📷"), ("video", "🎬"),
    ("movie", "🎬"), ("game", "🎮"), ("dog", "🐶"), ("cat", "🐱"), ("strong", "💪"), ("gym", "💪"),     ("question", "❓"), ("warning", "⚠️"), ("stop", "🛑"),
    ("cool", "😎"), ("wow", "😮"), ("crazy", "🤯"), ("magic", "✨"), ("eye", "👀"),     ("hello", "👋"), ("bye", "👋"), ("thanks", "🙏"), ("water", "💧"), ("rain", "🌧️"), ("snow", "❄️"),
    ("plane", "✈️"), ("travel", "✈️"), ("trip", "✈️"), ("health", "🩺"), ("doctor", "🩺"), ("science", "🔬"), ("ai", "🤖"),
    ("robot", "🤖"), ("computer", "💻"), ("code", "💻"), ("email", "📧"), ("lock", "🔒"), ("secret", "🤫"), ("goal", "🎯"),
    ("target", "🎯"), ("light", "💡"), ("power", "⚡"), ("fast", "⚡"), ("sleep", "😴"), ("tired", "😴"),
];

/// The emoji for a word (its core, or the core less a plural `s`).
pub fn emoji_for(text: &str) -> Option<&'static str> {
    let word = core(text);
    let singular = word.strip_suffix('s').unwrap_or(&word);
    EMOJI.iter().find(|(key, _)| *key == word || (singular.len() > 1 && *key == singular)).map(|(_, emoji)| *emoji)
}

fn is_emoji_word(text: &str) -> bool {
    EMOJI.iter().any(|(_, emoji)| *emoji == text)
}

/// One emoji after the first word that has one, in each caption that has
/// none yet; only the captions that changed come back.
pub fn add_emoji(captions: &[Caption]) -> Vec<Edited> {
    let mut out = Vec::new();
    for caption in captions {
        if caption.words.iter().any(|word| is_emoji_word(&word.text)) {
            continue;
        }
        let Some((index, emoji)) = caption.words.iter().enumerate().find_map(|(index, word)| emoji_for(&word.text).map(|emoji| (index, emoji)))
        else {
            continue;
        };
        let mut words = caption.words.clone();
        let end = words[index].end;
        words.insert(index + 1, Word { text: emoji.to_owned(), start: end, end, keyword: false });
        out.push(Edited { id: caption.id.clone(), content: content(&words), words });
    }
    out
}

/// The emoji words taken out again; only the captions that changed come back.
pub fn remove_emoji(captions: &[Caption]) -> Vec<Edited> {
    captions
        .iter()
        .filter(|caption| caption.words.iter().any(|word| is_emoji_word(&word.text)))
        .map(|caption| {
            let words: Vec<Word> = caption.words.iter().filter(|word| !is_emoji_word(&word.text)).cloned().collect();
            Edited { id: caption.id.clone(), content: content(&words), words }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn edits_match_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/captionedit/cases.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let captions: Vec<Caption> = serde_json::from_value(fixture["captions"].clone()).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let query = case["query"].as_str().unwrap_or("");
            let got = match case["op"].as_str().unwrap() {
                "find" => {
                    let want: Vec<Match> = serde_json::from_value(case["matches"].clone()).unwrap();
                    assert_eq!(find(&captions, query), want, "{name}");
                    continue;
                }
                "replace" => replace(&captions, query, case["replacement"].as_str().unwrap()),
                "highlight" => highlight(&captions, query),
                "addEmoji" => add_emoji(&captions),
                "removeEmoji" => {
                    let added = add_emoji(&captions);
                    let as_captions: Vec<Caption> = added.iter().map(|edited| Caption { id: edited.id.clone(), words: edited.words.clone() }).collect();
                    remove_emoji(&as_captions)
                }
                other => panic!("{other}"),
            };
            let want: Vec<Edited> = serde_json::from_value(case["edited"].clone()).unwrap();
            assert_eq!(got, want, "{name}");
        }
    }

    #[test]
    fn plurals_and_punctuation() {
        assert_eq!(emoji_for("Ideas!"), Some("💡"));
        assert_eq!(emoji_for("s"), None);
        assert_eq!(core("\"Plan,\""), "plan");
        assert_eq!(edges("\"Plan,\""), ("\"", ",\""));
        assert_eq!(edges("..."), ("...", ""));
    }
}

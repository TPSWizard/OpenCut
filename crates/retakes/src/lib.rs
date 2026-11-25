//! Retakes: when a speaker says a line, stops and says it again, the
//! earlier tries are cut and the last take kept. Works on a transcript's
//! timed words (any language with spaces between words), with no model:
//!
//! - The words are split into phrases at sentence ends (`.`, `?`, `!`,
//!   `…`, a dash) and at pauses of [`RetakeOptions::pause_seconds`] or more.
//! - **Retake:** a phrase is an earlier take of one of the next three
//!   phrases (starting within [`RetakeOptions::window_seconds`]) when both
//!   open with the same words (its first three, or all of a shorter one)
//!   and most of its words come back in order in the later one (longest
//!   common subsequence over its own length at least
//!   [`RetakeOptions::similarity`]). Short phrases between the two ("sorry,
//!   let me redo that"), and hesitations before the kept take's first
//!   word, go with it.
//! - **Restart:** inside a phrase, a run of words that starts again a few
//!   words later ("I think we should I think we should go") is a false
//!   start: the first try goes when its opening words repeat and nearly
//!   all of it comes back right after.
//!
//! Words are compared lower case with only their letters and digits;
//! hesitations ("um", "uh", "ähm", …) are skipped when comparing but go
//! with a cut. Each editor turns the words found into cuts with its
//! text-based editing (`editor::transcript_cuts`, the web's
//! `transcript/cuts.ts`). Checked against `fixtures/retakes/cases.json`.

use serde::{Deserialize, Serialize};

/// A timed word, timeline or source seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RetakeOptions {
    /// A pause at least this long ends a phrase.
    pub pause_seconds: f64,
    /// Share of an earlier take's words that must come back in the later one.
    pub similarity: f64,
    /// The later take starts at most this long after the earlier one.
    pub window_seconds: f64,
    /// Look for false starts inside phrases too.
    pub restarts: bool,
}

impl Default for RetakeOptions {
    fn default() -> Self {
        Self { pause_seconds: 0.7, similarity: 0.6, window_seconds: 30.0, restarts: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RetakeKind {
    /// A whole phrase said again later.
    Retake,
    /// A false start inside a phrase.
    Restart,
}

/// Words to cut: an earlier take and the take kept.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Retake {
    pub kind: RetakeKind,
    /// Indices into the words given, in time order.
    pub words: Vec<usize>,
    /// Index of the first word of the take that is kept.
    pub kept: usize,
    /// Share of the cut take's words found again in the kept one.
    pub similarity: f64,
    /// Seconds from the first cut word's start to the last one's end.
    pub start: f64,
    pub end: f64,
    /// The words cut, and the start of the take kept, as said.
    pub text: String,
    pub kept_text: String,
}

/// Hesitations skipped when comparing (the "auto" filler set of
/// `editor::fillers`, which a test there keeps in step).
pub const HESITATIONS: &[&str] = &[
    "um", "uh", "er", "erm", "ah", "hm", "mhm", "mm", "uhm", "ehm", "äh", "ähm", "öhm", "mh", "euh", "heu", "hum", "eh", "hã", "ahn",
    "ã",
];

/// Lower case, letters and digits only: "Don't," → "dont".
pub fn normalize(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

fn is_hesitation(token: &str) -> bool {
    let mut collapsed = String::new();
    let mut last = None;
    for c in token.chars() {
        if last != Some(c) {
            collapsed.push(c);
        }
        last = Some(c);
    }
    HESITATIONS.contains(&collapsed.as_str())
}

fn ends_phrase(text: &str) -> bool {
    let trimmed = text.trim_end_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == '»' || c == '”');
    trimmed.ends_with(['.', '?', '!', '…', '—', '–', '-'])
}

/// A phrase: positions `first..=last` in time order, and its compared
/// words as (token, position).
struct Phrase {
    first: usize,
    last: usize,
    tokens: Vec<(String, usize)>,
}

fn lcs(a: &[&str], b: &[&str]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if x == y { diagonal + 1 } else { above.max(row[j]) };
            diagonal = above;
        }
    }
    row[b.len()]
}

fn common_prefix(a: &[&str], b: &[&str]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// The retakes among the words, in time order, never overlapping.
pub fn find_retakes(words: &[Word], options: &RetakeOptions) -> Vec<Retake> {
    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by(|&a, &b| words[a].start.total_cmp(&words[b].start));
    let word = |position: usize| &words[order[position]];

    // Phrases.
    let mut phrases: Vec<Phrase> = Vec::new();
    let mut first = 0;
    for position in 0..order.len() {
        let next_pause = order.get(position + 1).map(|_| word(position + 1).start - word(position).end);
        let ends = ends_phrase(&word(position).text) || next_pause.is_none_or(|pause| pause >= options.pause_seconds);
        if ends {
            let tokens = (first..=position)
                .filter_map(|p| {
                    let token = normalize(&word(p).text);
                    (!token.is_empty() && !is_hesitation(&token)).then_some((token, p))
                })
                .collect();
            phrases.push(Phrase { first, last: position, tokens });
            first = position + 1;
        }
    }

    let text_of = |from: usize, to: usize| -> String {
        (from..=to).map(|p| word(p).text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
    };
    let kept_text = |from: usize| -> String {
        let to = (from + 7).min(order.len() - 1);
        text_of(from, to)
    };
    let make = |kind, from: usize, to: usize, kept: usize, similarity: f64| Retake {
        kind,
        words: (from..=to).map(|p| order[p]).collect(),
        kept: order[kept],
        similarity: (similarity * 1000.0).round() / 1000.0,
        start: word(from).start,
        end: (from..=to).map(|p| word(p).end).fold(f64::NEG_INFINITY, f64::max),
        text: text_of(from, to),
        kept_text: kept_text(kept),
    };

    // Whole phrases said again.
    let mut found: Vec<Retake> = Vec::new();
    let mut taken = vec![false; phrases.len()];
    for i in 0..phrases.len() {
        if taken[i] {
            continue;
        }
        let a: Vec<&str> = phrases[i].tokens.iter().map(|(t, _)| t.as_str()).collect();
        if a.len() < 2 {
            continue;
        }
        let start = word(phrases[i].first).start;
        for j in i + 1..phrases.len().min(i + 4) {
            if word(phrases[j].first).start - start > options.window_seconds {
                break;
            }
            let b: Vec<&str> = phrases[j].tokens.iter().map(|(t, _)| t.as_str()).collect();
            if b.len() < 2 || common_prefix(&a, &b) < a.len().min(3) {
                continue;
            }
            let similarity = lcs(&a, &b) as f64 / a.len() as f64;
            if similarity < options.similarity {
                continue;
            }
            // Short phrases between the two go too (and hesitations before
            // the kept take's first word); a long one stays.
            let kept = phrases[j].tokens[0].1;
            let between: usize = phrases[i + 1..j].iter().map(|p| p.tokens.len()).sum();
            let last = if between <= 8 { kept - 1 } else { phrases[i].last };
            for (k, flag) in taken.iter_mut().enumerate().take(j).skip(i) {
                if between <= 8 || k == i {
                    *flag = true;
                }
            }
            found.push(make(RetakeKind::Retake, phrases[i].first, last, kept, similarity));
            break;
        }
    }

    // False starts inside the phrases that stay.
    if options.restarts {
        for (index, phrase) in phrases.iter().enumerate() {
            if taken[index] {
                continue;
            }
            let t: Vec<&str> = phrase.tokens.iter().map(|(t, _)| t.as_str()).collect();
            let n = t.len();
            let mut i = 0;
            while i + 2 < n {
                let mut hit = None;
                let mut j = (i + 8).min(n - 2);
                while j >= i + 2 {
                    let length = j - i;
                    let prefix = common_prefix(&t[i..j], &t[j..]);
                    if prefix >= 2 && prefix >= length.min(3) {
                        let again = &t[j..(j + length + 2).min(n)];
                        let similarity = lcs(&t[i..j], again) as f64 / length as f64;
                        if similarity >= 0.75 {
                            hit = Some((j, similarity));
                            break;
                        }
                    }
                    j -= 1;
                }
                match hit {
                    Some((j, similarity)) => {
                        let from = phrase.tokens[i].1;
                        let kept = phrase.tokens[j].1;
                        found.push(make(RetakeKind::Restart, from, kept - 1, kept, similarity));
                        i = j;
                    }
                    None => i += 1,
                }
            }
        }
    }
    found.sort_by(|a, b| a.start.total_cmp(&b.start));
    found
}

/// Every word index the retakes cut, ascending.
pub fn retake_words(retakes: &[Retake]) -> Vec<usize> {
    let mut all: Vec<usize> = retakes.iter().flat_map(|r| r.words.iter().copied()).collect();
    all.sort_unstable();
    all.dedup();
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/retakes/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    #[test]
    fn retakes_match_the_reference() {
        for case in cases()["cases"].as_array().unwrap() {
            let words: Vec<Word> = serde_json::from_value(case["words"].clone()).unwrap();
            let options: RetakeOptions = serde_json::from_value(case["options"].clone()).unwrap();
            let got = find_retakes(&words, &options);
            let want = case["retakes"].as_array().unwrap();
            assert_eq!(got.len(), want.len(), "{}: {got:#?}", case["name"]);
            for (g, w) in got.iter().zip(want) {
                let kind = if g.kind == RetakeKind::Retake { "retake" } else { "restart" };
                assert_eq!(kind, w["kind"].as_str().unwrap(), "{}", case["name"]);
                let indices: Vec<usize> = serde_json::from_value(w["words"].clone()).unwrap();
                assert_eq!(g.words, indices, "{}: {g:#?}", case["name"]);
                assert_eq!(g.kept, w["kept"].as_u64().unwrap() as usize, "{}", case["name"]);
                assert!((g.similarity - w["similarity"].as_f64().unwrap()).abs() < 1e-9, "{}: {g:#?}", case["name"]);
            }
            let all: Vec<usize> = serde_json::from_value(case["cut"].clone()).unwrap();
            assert_eq!(retake_words(&got), all, "{}", case["name"]);
        }
    }

    #[test]
    fn words_compare_by_letters_and_skip_hesitations() {
        assert_eq!(normalize("Don't,"), "dont");
        assert_eq!(normalize("Ärger!"), "ärger");
        assert!(is_hesitation("ummm") && is_hesitation("ähm") && !is_hesitation("hmmmmmmm-no"));
        assert!(ends_phrase("plan.") && ends_phrase("plan-") && ends_phrase("so…\"") && !ends_phrase("plan,"));
    }
}

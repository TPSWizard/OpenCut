//! Caption keywords: the words of a caption that matter most, drawn in the
//! keyword colour and a little larger (`caption.keywordColor`,
//! `caption.keywordScale`). A word is a keyword when its `keyword` flag is
//! set; [`pick_keyword_indices`] is the local pick, the same as the web
//! app's `src/captions/keywords.ts` (both are checked against
//! `fixtures/caption-keywords/cases.json`): each word is stripped of the
//! punctuation around it and scored — anything with a digit first, then
//! names (a capital letter not at the start of a sentence), then other
//! words of four or more letters by length; stop words and shorter words
//! never count. A caption of n words keeps its best round(n / 6) words,
//! between one and three, earlier words winning ties; a one-word caption
//! keeps none.

/// Common English words that never make a keyword (the web app's `STOP_WORDS`).
pub const STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "also", "am", "an", "and", "any", "are", "around", "as", "at",
    "be", "because", "been", "before", "being", "below", "between", "both", "but", "by", "can", "could", "did", "do", "does",
    "doing", "done", "down", "during", "each", "even", "ever", "every", "few", "for", "from", "further", "get", "gets", "getting",
    "go", "goes", "going", "gonna", "got", "had", "has", "have", "having", "he", "her", "here", "hers", "herself", "him",
    "himself", "his", "how", "i", "if", "in", "into", "is", "it", "its", "itself", "just", "kind", "know", "like", "lot", "make",
    "many", "maybe", "me", "might", "more", "most", "much", "must", "my", "myself", "need", "never", "no", "nor", "not", "now",
    "of", "off", "okay", "on", "once", "one", "only", "or", "other", "our", "ours", "ourselves", "out", "over", "own", "really",
    "right", "said", "same", "say", "says", "see", "she", "should", "so", "some", "something", "still", "such", "sure", "take",
    "than", "that", "the", "their", "theirs", "them", "themselves", "then", "there", "these", "they", "thing", "things", "think",
    "this", "those", "through", "to", "too", "under", "until", "up", "us", "very", "want", "was", "way", "we", "well", "were",
    "what", "when", "where", "which", "while", "who", "whom", "why", "will", "with", "would", "yeah", "yes", "yet", "you",
    "your", "yours", "yourself", "yourselves",
];

/// The default keyword colour and size (the web app's `DEFAULT_CAPTION_STYLE`).
pub const KEYWORD_COLOR_DEFAULT: &str = "#3ee07a";
pub const KEYWORD_SCALE_DEFAULT: f64 = 1.25;
pub const KEYWORD_SCALE_MAX: f64 = 2.0;

/// Closing marks a sentence-ending word may carry after its full stop.
const CLOSERS: [char; 5] = ['"', '\'', '\u{201d}', '\u{2019}', ')'];

/// The word without the punctuation around it.
pub fn keyword_core(text: &str) -> &str {
    text.trim_matches(|c: char| !c.is_alphanumeric())
}

/// How much a word stands out, or 0 when it can never be a keyword;
/// `previous` is the word before it in the caption.
pub fn keyword_score(text: &str, previous: Option<&str>) -> usize {
    let core = keyword_core(text);
    let length = core.chars().count();
    if length == 0 {
        return 0;
    }
    if core.chars().any(char::is_numeric) {
        return 1000 + length;
    }
    if STOP_WORDS.contains(&core.to_lowercase().as_str()) {
        return 0;
    }
    let sentence_start = previous.is_none_or(|previous| previous.trim_end_matches(CLOSERS).ends_with(['.', '!', '?']));
    if !sentence_start && core.chars().next().is_some_and(char::is_uppercase) {
        return 500 + length;
    }
    if length >= 4 { length } else { 0 }
}

/// How many keywords a caption of `count` words keeps.
pub fn keyword_budget(count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    ((count as f64 / 6.0).round() as usize).clamp(1, 3)
}

/// The indices of the caption's keywords, in order.
pub fn pick_keyword_indices<S: AsRef<str>>(words: &[S]) -> Vec<usize> {
    let mut scored: Vec<(usize, usize)> = words
        .iter()
        .enumerate()
        .map(|(index, word)| (index, keyword_score(word.as_ref(), index.checked_sub(1).map(|previous| words[previous].as_ref()))))
        .filter(|(_, score)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut picked: Vec<usize> = scored.into_iter().take(keyword_budget(words.len())).map(|(index, _)| index).collect();
    picked.sort_unstable();
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_shared_cases_like_the_web() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/caption-keywords/cases.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 5);
        for case in cases {
            let words: Vec<&str> = case["words"].as_array().unwrap().iter().map(|word| word.as_str().unwrap()).collect();
            let expected: Vec<usize> =
                case["keywords"].as_array().unwrap().iter().map(|index| index.as_u64().unwrap() as usize).collect();
            assert_eq!(pick_keyword_indices(&words), expected, "{}", words.join(" "));
        }
    }

    #[test]
    fn the_flag_is_saved_only_when_set_and_a_patch_sets_it() {
        let plain: crate::CaptionWord = serde_json::from_str(r#"{"text":"hi","start":0,"end":10}"#).unwrap();
        assert!(!plain.keyword, "older files load as plain words");
        assert!(!serde_json::to_string(&plain).unwrap().contains("keyword"));
        let keyed = crate::CaptionWord { keyword: true, ..plain.clone() };
        assert!(serde_json::to_string(&keyed).unwrap().contains(r#""keyword":true"#));

        let mut project = crate::Project::new("Keywords");
        let mut text = crate::Element::text("x y z", time::MediaTime::ZERO);
        text.words = ["x", "y", "z"].iter().map(|word| crate::CaptionWord { text: (*word).into(), ..plain.clone() }).collect();
        let id = text.id.clone();
        crate::apply(&mut project, &crate::Command::InsertElement { element: text, placement: crate::TrackPlacement::Auto }).unwrap();
        let patch = |keywords: Vec<usize>| crate::Command::UpdateElement {
            element_id: id.clone(),
            patch: crate::ElementPatch { keywords: Some(keywords), ..Default::default() },
        };
        crate::apply(&mut project, &patch(vec![0, 2, 9])).unwrap();
        let flags = |project: &crate::Project| -> Vec<bool> { project.tracks[0].elements[0].words.iter().map(|word| word.keyword).collect() };
        assert_eq!(flags(&project), vec![true, false, true]);
        crate::apply(&mut project, &patch(vec![1])).unwrap();
        assert_eq!(flags(&project), vec![false, true, false]);
        let keywords = crate::caption_keywords(&project.tracks[0].elements[0].params, &project.tracks[0].elements[0].words).unwrap();
        assert_eq!((keywords.indices, keywords.scale), (vec![1], 1.25));
        assert_eq!(keywords.color, crate::parse_hex_color(KEYWORD_COLOR_DEFAULT).unwrap());
    }

    #[test]
    fn budgets_one_to_three() {
        let budgets: Vec<usize> = [1, 2, 6, 9, 12, 15, 40].into_iter().map(keyword_budget).collect();
        assert_eq!(budgets, vec![0, 1, 1, 2, 2, 3, 3]);
    }
}

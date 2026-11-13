//! Voice-over from a script: the script is cut into lines (sentences, long
//! ones broken at a comma or a space) that a text-to-speech voice reads one
//! by one, with a short pause after each sentence and a longer one at a
//! paragraph break. Knowing where each line starts in the joined audio
//! gives captions timed line by line. The web app's
//! `src/voiceover/script.ts` is the same arithmetic.

/// Lines longer than this (characters) are broken up; voices read long
/// inputs worse, and captions want short lines.
pub const MAX_LINE_CHARS: usize = 180;
/// Silence after a sentence, seconds.
pub const SENTENCE_PAUSE: f64 = 0.25;
/// Silence after a paragraph (a blank line in the script), seconds.
pub const PARAGRAPH_PAUSE: f64 = 0.6;

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptLine {
    pub text: String,
    /// Silence after this line (0 after the last).
    pub pause_after: f64,
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？')
}

/// Sentences of one paragraph (whitespace already collapsed).
fn sentences(paragraph: &str) -> Vec<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let next = chars.get(i + 1).copied();
        // A run of enders ("?!", "...") ends where the next character is a space or the text ends,
        // or straight after a full-width ender.
        let ends = is_sentence_end(c)
            && (next.is_none_or(|n| n == ' ') || matches!(c, '。' | '！' | '？'))
            && !next.is_some_and(is_sentence_end);
        if ends {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_owned());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_owned());
    }
    out
}

/// Breaks a line longer than `max` at the comma (or semicolon) nearest its
/// middle, else the space nearest its middle, until every piece fits.
fn break_long(line: &str, max: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= max {
        return vec![line.to_owned()];
    }
    let middle = chars.len() / 2;
    let nearest = |accept: &dyn Fn(usize) -> bool| -> Option<usize> {
        (1..chars.len() - 1).filter(|&i| accept(i)).min_by_key(|&i| i.abs_diff(middle))
    };
    let cut = nearest(&|i| matches!(chars[i], ',' | ';' | '，' | '；') && chars.get(i + 1) == Some(&' '))
        .map(|i| i + 1)
        .or_else(|| nearest(&|i| chars[i] == ' '))
        .unwrap_or(middle);
    let left: String = chars[..cut].iter().collect();
    let right: String = chars[cut..].iter().collect();
    let mut out = break_long(left.trim(), max);
    out.extend(break_long(right.trim(), max));
    out
}

/// The script as lines to read, with the pause after each.
pub fn split_script(script: &str, max_chars: usize) -> Vec<ScriptLine> {
    let normalized = script.replace("\r\n", "\n");
    let paragraphs: Vec<String> = normalized
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect();
    let mut lines = Vec::new();
    for paragraph in &paragraphs {
        let pieces: Vec<String> =
            sentences(paragraph).iter().flat_map(|sentence| break_long(sentence, max_chars.max(20))).collect();
        let count = pieces.len();
        for (index, text) in pieces.into_iter().enumerate() {
            let pause_after = if index + 1 == count { PARAGRAPH_PAUSE } else { SENTENCE_PAUSE };
            lines.push(ScriptLine { text, pause_after });
        }
    }
    if let Some(last) = lines.last_mut() {
        last.pause_after = 0.0;
    }
    lines
}

/// Where each line plays in the joined audio, `(start, end)` in seconds,
/// given how long each line's speech lasts; and the total length.
pub fn layout_lines(lines: &[ScriptLine], durations: &[f64]) -> (Vec<(f64, f64)>, f64) {
    let mut cursor = 0.0;
    let mut spans = Vec::with_capacity(lines.len());
    for (line, duration) in lines.iter().zip(durations) {
        let end = cursor + duration.max(0.0);
        spans.push((cursor, end));
        cursor = end + line.pause_after;
    }
    (spans, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same script and expectations are in the web app's
    /// `src/voiceover/__tests__/script.test.ts`.
    #[test]
    fn scripts_split_into_lines_with_pauses() {
        let script = "Welcome back to the channel!  Today we cook rice.\r\n\r\nFirst, wash the rice three times... Then soak it, drain it, and let it rest for twenty minutes while you chop the onions, the garlic, the ginger and the spring onions for the sauce that goes on top.\n\n你好。今天做饭！";
        let lines = split_script(script, 80);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Welcome back to the channel!",
                "Today we cook rice.",
                "First, wash the rice three times...",
                "Then soak it, drain it,",
                "and let it rest for twenty minutes while you chop the onions,",
                "the garlic, the ginger and the spring onions for the sauce that goes on top.",
                "你好。",
                "今天做饭！",
            ]
        );
        let pauses: Vec<f64> = lines.iter().map(|l| l.pause_after).collect();
        assert_eq!(pauses, vec![0.25, 0.6, 0.25, 0.25, 0.25, 0.6, 0.25, 0.0]);
        assert!(split_script("  \n\n ", 80).is_empty());
    }

    #[test]
    fn lines_are_laid_out_with_their_pauses() {
        let lines = split_script("One. Two.\n\nThree.", 80);
        let (spans, total) = layout_lines(&lines, &[1.0, 2.0, 0.5]);
        assert_eq!(spans, vec![(0.0, 1.0), (1.25, 3.25), (3.85, 4.35)]);
        assert!((total - 4.35).abs() < 1e-9);
    }
}

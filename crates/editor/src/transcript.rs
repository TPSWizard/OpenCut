//! Text-based editing: deleting words from a clip's transcript cuts the
//! clip. [`transcript_cuts`] turns the deleted words into the timeline
//! stretches to remove, and [`retime_captions`] keeps the captions in step
//! with the cut: words inside a removed stretch go, and with a ripple over
//! every track the words after it move left with the picture, even inside a
//! caption that straddles the cut (the ripple itself only moves captions
//! that start after it). The web app's `src/transcript/cuts.ts` is the twin.

use std::collections::HashSet;

use time::MediaTime;

use crate::captions::TimedWord;
use crate::silence::RippleScope;
use crate::document::{CaptionWord, ElementKind, Project};

/// Seconds kept around a deleted word that has no neighbour on that side.
pub const TRANSCRIPT_EDGE_PADDING: f64 = 0.08;

/// The stretches `(start, end)` in seconds to cut for the deleted words
/// (indices into `words`), inside the clip's span `[from, to]`. Each run of
/// deleted words is cut from halfway through the pause before it to halfway
/// through the pause after it, so what is left keeps a natural pause; at the
/// ends of the transcript a little padding is cut instead.
pub fn transcript_cuts(words: &[TimedWord], deleted: &[usize], from: f64, to: f64) -> Vec<(f64, f64)> {
    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by(|&a, &b| words[a].start.total_cmp(&words[b].start));
    let gone: HashSet<usize> = deleted.iter().copied().filter(|index| *index < words.len()).collect();
    let mut cuts: Vec<(f64, f64)> = Vec::new();
    let mut index = 0;
    while index < order.len() {
        if !gone.contains(&order[index]) {
            index += 1;
            continue;
        }
        let mut last = index;
        while last + 1 < order.len() && gone.contains(&order[last + 1]) {
            last += 1;
        }
        let first_word = &words[order[index]];
        let last_word = &words[order[last]];
        let start = match index.checked_sub(1).map(|previous| &words[order[previous]]) {
            Some(previous) => {
                let pause_start = previous.end.min(first_word.start);
                (pause_start + first_word.start) / 2.0
            }
            None => first_word.start - TRANSCRIPT_EDGE_PADDING,
        };
        let end = match order.get(last + 1).map(|next| &words[*next]) {
            Some(next) => {
                let pause_end = next.start.max(last_word.end);
                (last_word.end + pause_end) / 2.0
            }
            None => last_word.end + TRANSCRIPT_EDGE_PADDING,
        };
        let (start, end) = (start.max(from), end.min(to));
        if end > start {
            match cuts.last_mut() {
                Some(previous) if start <= previous.1 => previous.1 = previous.1.max(end),
                _ => cuts.push((start, end)),
            }
        }
        index = last + 1;
    }
    cuts
}

/// Brings the timed captions in line with cutting `ranges` (timeline time)
/// out of the picture: a word whose middle falls in a removed stretch is
/// dropped, and with [`RippleScope::All`] the words after the stretch move
/// left by its length. Captions left with no words go. Captions entirely
/// before or after every stretch are untouched (the ripple moves the later
/// ones), as are locked tracks and text without word timings. Run it before
/// the cut, on the same ranges.
pub fn retime_captions(project: &mut Project, ranges: &[(MediaTime, MediaTime)], ripple: RippleScope) -> usize {
    let mut sorted: Vec<(MediaTime, MediaTime)> = ranges.iter().copied().filter(|(start, end)| end > start).collect();
    sorted.sort_by_key(|(start, _)| *start);
    let shifting = ripple == RippleScope::All;
    let mut changed = 0;
    // Latest first, so earlier stretches keep their positions.
    for (cut_start, cut_end) in sorted.into_iter().rev() {
        let shift = cut_end - cut_start;
        for track in project.tracks.iter_mut() {
            if track.locked {
                continue;
            }
            let mut emptied: Vec<String> = Vec::new();
            for element in track.elements.iter_mut() {
                if element.kind != ElementKind::Text || element.words.is_empty() {
                    continue;
                }
                let (start, end) = (element.start_time, element.end_time());
                if end <= cut_start || start >= cut_end {
                    continue;
                }
                let new_start = if shifting && start >= cut_start { cut_start } else { start };
                let new_end = if end <= cut_end {
                    cut_start.max(new_start)
                } else if shifting {
                    end - shift
                } else {
                    end
                };
                let mut words: Vec<CaptionWord> = Vec::new();
                for word in &element.words {
                    let (word_start, word_end) = (start + word.start, start + word.end);
                    let middle = MediaTime::from_ticks((word_start.as_ticks() + word_end.as_ticks()) / 2);
                    if middle >= cut_start && middle < cut_end {
                        continue;
                    }
                    let (word_start, word_end) = if middle >= cut_end && shifting {
                        (word_start - shift, word_end - shift)
                    } else if middle < cut_start && shifting {
                        (word_start, word_end.min(cut_start))
                    } else {
                        (word_start, word_end)
                    };
                    let relative = |time: MediaTime| (time - new_start).max(MediaTime::ZERO).min(new_end - new_start);
                    words.push(CaptionWord { text: word.text.clone(), start: relative(word_start), end: relative(word_end) });
                }
                changed += 1;
                if words.is_empty() || new_end <= new_start {
                    emptied.push(element.id.clone());
                    continue;
                }
                let content = words.iter().map(|word| word.text.trim()).collect::<Vec<_>>().join(" ");
                element.params.set("content", content);
                element.words = words;
                element.start_time = new_start;
                element.duration = new_end - new_start;
            }
            track.elements.retain(|element| !emptied.contains(&element.id));
            track.sort_elements();
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord { text: text.to_owned(), start, end }
    }

    fn sentence() -> Vec<TimedWord> {
        vec![
            word("So", 1.0, 1.2),
            word("the", 1.4, 1.5),
            word("plan", 1.5, 1.9),
            word("is", 2.1, 2.3),
            word("simple.", 2.5, 3.0),
        ]
    }

    #[test]
    fn cuts_from_pause_middle_to_pause_middle() {
        let words = sentence();
        // "the plan": pause before is 1.2..1.4, after is 1.9..2.1.
        let cuts = transcript_cuts(&words, &[1, 2], 0.0, 10.0);
        assert_eq!(cuts.len(), 1);
        assert!((cuts[0].0 - 1.3).abs() < 1e-9 && (cuts[0].1 - 2.0).abs() < 1e-9, "{cuts:?}");
        // Separate runs make separate cuts; out-of-range indices are ignored.
        assert_eq!(transcript_cuts(&words, &[1, 3, 99], 0.0, 10.0).len(), 2);
        assert!(transcript_cuts(&words, &[], 0.0, 10.0).is_empty());
    }

    #[test]
    fn pads_the_ends_and_stays_inside_the_clip() {
        let words = sentence();
        let cuts = transcript_cuts(&words, &[0], 0.95, 10.0);
        assert!((cuts[0].0 - 0.95).abs() < 1e-9 && (cuts[0].1 - 1.3).abs() < 1e-9, "{cuts:?}");
        let cuts = transcript_cuts(&words, &[4], 0.0, 10.0);
        assert!((cuts[0].0 - 2.4).abs() < 1e-9 && (cuts[0].1 - 3.08).abs() < 1e-9, "{cuts:?}");
        // Every word: one cut over the lot.
        let cuts = transcript_cuts(&words, &[0, 1, 2, 3, 4], 0.0, 10.0);
        assert_eq!(cuts.len(), 1);
        // Order of the input does not matter.
        let mut shuffled = words.clone();
        shuffled.reverse();
        let cuts = transcript_cuts(&shuffled, &[2, 3], 0.0, 10.0);
        assert!((cuts[0].0 - 1.3).abs() < 1e-9 && (cuts[0].1 - 2.0).abs() < 1e-9, "{cuts:?}");
    }
}

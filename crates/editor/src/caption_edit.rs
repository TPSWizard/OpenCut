//! Caption find and replace, highlight by search and auto emoji (row 94)
//! across a project's captions; the shared `captionedit` crate does the
//! words, this turns its result into one undo step.

use time::MediaTime;

use crate::{CaptionWord, Command, ElementKind, ElementPatch, Params, Project};

/// What to do to the captions.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptionEdit {
    /// Replace every match of the search with `replacement` (empty removes it).
    Replace { query: String, replacement: String },
    /// Make every match a keyword.
    Highlight { query: String },
    AddEmoji,
    RemoveEmoji,
}

/// The project's captions (text with timed words) on unlocked tracks, in
/// timeline order, as `captionedit` reads them.
fn captions(project: &Project) -> Vec<captionedit::Caption> {
    let mut found: Vec<(MediaTime, captionedit::Caption)> = project
        .tracks
        .iter()
        .filter(|track| !track.locked)
        .flat_map(|track| &track.elements)
        .filter(|element| matches!(element.kind, ElementKind::Text) && !element.words.is_empty())
        .map(|element| {
            let words = element
                .words
                .iter()
                .map(|word| captionedit::Word { text: word.text.clone(), start: word.start.as_ticks(), end: word.end.as_ticks(), keyword: word.keyword })
                .collect();
            (element.start_time, captionedit::Caption { id: element.id.clone(), words })
        })
        .collect();
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, caption)| caption).collect()
}

/// How many places the search matches across the captions.
pub fn caption_match_count(project: &Project, query: &str) -> usize {
    captionedit::find(&captions(project), query).len()
}

/// The command for a caption edit and how many captions it changes;
/// `None` when nothing changes.
pub fn caption_edit_command(project: &Project, edit: &CaptionEdit) -> Option<(Command, usize)> {
    let captions = captions(project);
    let edited = match edit {
        CaptionEdit::Replace { query, replacement } => captionedit::replace(&captions, query, replacement),
        CaptionEdit::Highlight { query } => captionedit::highlight(&captions, query),
        CaptionEdit::AddEmoji => captionedit::add_emoji(&captions),
        CaptionEdit::RemoveEmoji => captionedit::remove_emoji(&captions),
    };
    if edited.is_empty() {
        return None;
    }
    let count = edited.len();
    let commands = edited
        .into_iter()
        .map(|caption| {
            let mut params = Params::new();
            params.set("content", caption.content);
            let words = caption
                .words
                .into_iter()
                .map(|word| CaptionWord {
                    text: word.text,
                    start: MediaTime::from_ticks(word.start),
                    end: MediaTime::from_ticks(word.end),
                    keyword: word.keyword,
                })
                .collect();
            Command::UpdateElement { element_id: caption.id, patch: ElementPatch { params: Some(params), words: Some(words), ..Default::default() } }
        })
        .collect();
    Some((Command::Batch(commands), count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, TrackPlacement, apply};

    #[test]
    fn replace_highlight_and_emoji_rewrite_captions_in_one_step() {
        let mut project = Project::new("Captions");
        for (start, text) in [(0.0, "Our plan is simple."), (3.0, "The plan costs money.")] {
            let mut element = Element::text(text, MediaTime::from_seconds_f64(start).unwrap());
            element.words = text
                .split_whitespace()
                .enumerate()
                .map(|(index, word)| CaptionWord {
                    text: word.to_owned(),
                    start: MediaTime::from_ticks(index as i64 * 60000),
                    end: MediaTime::from_ticks(index as i64 * 60000 + 50000),
                    keyword: false,
                })
                .collect();
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        assert_eq!(caption_match_count(&project, "plan"), 2);
        let (command, changed) =
            caption_edit_command(&project, &CaptionEdit::Replace { query: "plan".into(), replacement: "big idea".into() }).unwrap();
        assert_eq!(changed, 2);
        apply(&mut project, &command).unwrap();
        let texts: Vec<String> = project.tracks.iter().flat_map(|track| &track.elements).map(|element| element.params.text("content", "").into_owned()).collect();
        assert!(texts.contains(&"Our big idea is simple.".to_owned()), "{texts:?}");
        let (command, _) = caption_edit_command(&project, &CaptionEdit::AddEmoji).unwrap();
        apply(&mut project, &command).unwrap();
        let texts: Vec<String> = project.tracks.iter().flat_map(|track| &track.elements).map(|element| element.params.text("content", "").into_owned()).collect();
        assert!(texts.iter().any(|text| text.contains("idea 💡")), "{texts:?}");
        let (command, _) = caption_edit_command(&project, &CaptionEdit::Highlight { query: "big idea".into() }).unwrap();
        apply(&mut project, &command).unwrap();
        let keywords = project.tracks.iter().flat_map(|track| &track.elements).flat_map(|element| &element.words).filter(|word| word.keyword).count();
        assert_eq!(keywords, 4);
        assert!(caption_edit_command(&project, &CaptionEdit::Replace { query: "nowhere".into(), replacement: "x".into() }).is_none());
    }
}

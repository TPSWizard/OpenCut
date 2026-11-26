//! Subtitle import (row 95): an SRT or WebVTT file's cues as captions on
//! a new text track, read by the shared `subtitleparse` crate (the web
//! editor reads them the same way). Word timings in a WebVTT file become
//! the captions' timed words.

use std::path::Path;

use crate::{CaptionChunk, Command, Project, TimedWord, TrackKind, caption_element};

/// The command that puts the cues on a new text track named `name`, and
/// how many captions it adds; `None` when there are none.
pub fn subtitle_import_command(project: &Project, parsed: &subtitleparse::Parsed, name: &str) -> Option<(Command, usize)> {
    if parsed.cues.is_empty() {
        return None;
    }
    let canvas = project.settings.canvas;
    let elements: Vec<_> = parsed
        .cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            let chunk = CaptionChunk {
                text: cue.text.clone(),
                start: cue.start,
                duration: cue.end - cue.start,
                words: cue.words.iter().map(|word| TimedWord { text: word.text.clone(), start: word.start, end: word.end }).collect(),
            };
            let mut element = caption_element(index, &chunk, canvas);
            if cue.words.is_empty() {
                // Plain subtitles: no spoken-word highlight to show.
                element.params.set("caption.highlight", "none");
            }
            element
        })
        .collect();
    let count = elements.len();
    Some((Command::InsertOnNewTrack { kind: TrackKind::Text, name: name.to_owned(), elements }, count))
}

/// Reads an `.srt` or `.vtt` file (other names are sniffed from the text).
pub fn read_subtitle_file(path: &Path) -> Result<subtitleparse::Parsed, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let format = path.file_name().and_then(|name| name.to_str()).and_then(subtitleparse::Format::from_name);
    Ok(subtitleparse::parse(&text, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply;

    #[test]
    fn a_vtt_file_becomes_timed_captions() {
        let mut project = Project::new("Import");
        let parsed = subtitleparse::parse("WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nhello <00:00:02.600>world\n\n00:00:04.000 --> 00:00:05.000\nplain line\n", None);
        let (command, count) = subtitle_import_command(&project, &parsed, "Subtitles").unwrap();
        assert_eq!(count, 2);
        apply(&mut project, &command).unwrap();
        let track = project.tracks.iter().find(|track| track.name == "Subtitles").unwrap();
        let first = &track.elements[0];
        assert_eq!(first.params.text("content", ""), "hello world");
        assert_eq!(first.words.len(), 2);
        assert_eq!(first.words[1].start, time::MediaTime::from_seconds_f64(1.6).unwrap(), "relative to the caption");
        assert!(track.elements[1].words.is_empty());
        assert_eq!(track.elements[1].params.text("caption.highlight", ""), "none");
        assert!(subtitle_import_command(&project, &subtitleparse::Parsed::default(), "x").is_none());
    }
}

//! Saved markers on the project's timeline (the web app's bookmarks: a
//! time, an optional note, colour and length), the elements a time range
//! covers, and markers written as YouTube-style chapters.

use serde::{Deserialize, Serialize};
use time::MediaTime;

use crate::document::Project;

/// The colour markers get when none is chosen (the web app's default).
pub const DEFAULT_MARKER_COLOR: &str = "#009dff";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub time: MediaTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// A marker can cover a stretch of time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<MediaTime>,
}

impl Marker {
    pub fn at(time: MediaTime) -> Self {
        Self { time, note: None, color: None, duration: None }
    }
}

/// Sorted by time, one marker per time (the later one in the list wins).
pub fn normalize_markers(markers: &[Marker]) -> Vec<Marker> {
    let mut sorted: Vec<Marker> = Vec::with_capacity(markers.len());
    for marker in markers {
        if let Some(existing) = sorted.iter_mut().find(|m| m.time == marker.time) {
            *existing = marker.clone();
        } else {
            sorted.push(Marker { time: marker.time.max(MediaTime::ZERO), ..marker.clone() });
        }
    }
    sorted.sort_by_key(|marker| marker.time);
    sorted
}

/// Elements on unlocked tracks that overlap `[from, to)`.
pub fn elements_in_range(project: &Project, from: MediaTime, to: MediaTime) -> Vec<String> {
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    project
        .tracks
        .iter()
        .filter(|track| !track.locked)
        .flat_map(|track| track.elements.iter())
        .filter(|element| element.start_time < to && element.end_time() > from)
        .map(|element| element.id.clone())
        .collect()
}

fn chapter_time(seconds: f64, with_hours: bool) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if with_hours {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

/// The markers as YouTube chapters: one `MM:SS Title` line each, starting
/// at 00:00 (an "Intro" line is added when the first marker is later).
/// Markers without a note are called "Chapter N".
pub fn chapters_text(markers: &[Marker]) -> String {
    let markers = normalize_markers(markers);
    let with_hours = markers.iter().any(|marker| marker.time.to_seconds_f64() >= 3600.0);
    let mut lines = Vec::new();
    if markers.first().is_none_or(|marker| marker.time.to_seconds_f64() >= 1.0) {
        lines.push(format!("{} Intro", chapter_time(0.0, with_hours)));
    }
    for (index, marker) in markers.iter().enumerate() {
        let title = marker
            .note
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Chapter {}", index + 1));
        lines.push(format!("{} {title}", chapter_time(marker.time.to_seconds_f64(), with_hours)));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Element, TrackPlacement, apply};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn markers_sort_and_write_chapters() {
        let markers = vec![
            Marker { note: Some("Setup".into()), ..Marker::at(seconds(75.0)) },
            Marker::at(seconds(12.5)),
            Marker { note: Some("Replaced".into()), ..Marker::at(seconds(12.5)) },
        ];
        let sorted = normalize_markers(&markers);
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0].note.as_deref(), Some("Replaced"));
        assert_eq!(chapters_text(&markers), "00:00 Intro\n00:12 Replaced\n01:15 Setup");
        let long = vec![Marker::at(MediaTime::ZERO), Marker { note: Some(" End ".into()), ..Marker::at(seconds(3725.0)) }];
        assert_eq!(chapters_text(&long), "0:00:00 Chapter 1\n1:02:05 End");
        let json = serde_json::to_string(&Marker::at(seconds(1.0))).unwrap();
        assert_eq!(json, r#"{"time":120000}"#);
    }

    #[test]
    fn ranges_select_what_they_cover_on_unlocked_tracks() {
        let mut project = Project::new("Range");
        let mut add = |start: f64, length: f64| {
            let mut element = Element::text("t", seconds(start));
            element.duration = seconds(length);
            let outcome = apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
            outcome.element_refs()[0].element_id.clone()
        };
        let a = add(0.0, 2.0);
        let b = add(3.0, 2.0);
        let c = add(6.0, 2.0);
        let picked = elements_in_range(&project, seconds(4.0), seconds(1.0));
        assert_eq!(picked, vec![a.clone(), b.clone()]);
        assert!(!elements_in_range(&project, seconds(5.0), seconds(6.0)).contains(&c), "touching is not overlapping");
        let track_id = project.locate(&a).map(|(track, _)| project.tracks[track].id.clone()).unwrap();
        apply(&mut project, &Command::SetTrackLocked { track_id, locked: true }).unwrap();
        assert!(elements_in_range(&project, seconds(0.0), seconds(10.0)).is_empty());
    }
}

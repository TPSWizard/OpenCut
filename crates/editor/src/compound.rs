//! Compound clips: one clip on a video track holding other clips, each in
//! the compound's own time (0 is the compound's content start). The
//! compound can be moved, trimmed and split like a media clip, its
//! `source_duration` being the content's length. Renderers and the audio
//! mix never see it: [`expand_compounds`] swaps every compound for its
//! children, clipped to the compound's visible window, on tracks of their
//! own placed where the compound sits.
//!
//! The JSON shape is shared with the web app: `{ "type": "compound",
//! "tracks": [{ id, name, type, muted, hidden, elements }] }`.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use time::MediaTime;

use crate::document::{Element, ElementKind, Lane, Project, Track, TrackKind};

/// A track inside a compound clip (the web app's track shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompoundTrack {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: TrackKind,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub elements: Vec<Element>,
}

/// How deep compounds may nest before expansion stops.
const MAX_NESTING: usize = 8;

/// Whether any track holds a compound clip.
pub fn has_compounds(tracks: &[Track]) -> bool {
    tracks.iter().any(|track| track.elements.iter().any(|element| matches!(element.kind, ElementKind::Compound { .. })))
}

/// The ids of the clips grouped with `element_id` (itself included), in
/// timeline order; just itself when it is not grouped.
pub fn group_members(project: &Project, element_id: &str) -> Vec<String> {
    let Some((_, element)) = project.element(element_id) else { return Vec::new() };
    let Some(group) = element.group_id.as_deref() else { return vec![element_id.to_owned()] };
    let mut members: Vec<&Element> = project
        .tracks
        .iter()
        .flat_map(|track| track.elements.iter())
        .filter(|candidate| candidate.group_id.as_deref() == Some(group))
        .collect();
    members.sort_by_key(|member| member.start_time);
    members.into_iter().map(|member| member.id.clone()).collect()
}

/// Number of clips inside a compound (its direct children).
pub fn compound_child_count(element: &Element) -> usize {
    match &element.kind {
        ElementKind::Compound { tracks } => tracks.iter().map(|track| track.elements.len()).sum(),
        _ => 0,
    }
}

/// `element` with `cut_start` taken off its start and `cut_end` off its end
/// (in its own time): the same source frames stay at the same timeline
/// times, keyframes, caption words and ramp keys follow, and speed
/// changes and reverse are respected. The start time is left as is.
pub fn clip_element(element: &Element, cut_start: MediaTime, cut_end: MediaTime) -> Element {
    let cut_start = cut_start.max(MediaTime::ZERO);
    let cut_end = cut_end.max(MediaTime::ZERO);
    if cut_start == MediaTime::ZERO && cut_end == MediaTime::ZERO {
        return element.clone();
    }
    let mut clipped = element.clone();
    let duration = element.duration;
    let new_duration = (duration - cut_start - cut_end).max(MediaTime::ZERO);
    let seconds = |time: MediaTime| time.to_seconds_f64();
    let ticks = |value: f64| MediaTime::from_seconds_f64(value.max(0.0)).unwrap_or(MediaTime::ZERO);
    // How much more of the source the clip starts into.
    let advance = match &element.retime {
        Some(retime) if !retime.is_identity() => {
            if retime.reverse {
                // Backwards: the window's start is what the clip shows last,
                // so cutting the end moves it on.
                ticks(retime.forward(seconds(duration)) - retime.forward(seconds(duration - cut_end)))
            } else {
                ticks(retime.forward(seconds(cut_start)))
            }
        }
        _ => cut_start,
    };
    clipped.trim_start = element.trim_start + advance;
    clipped.duration = new_duration;
    if let Some(source) = element.source_duration {
        let span = match &clipped.retime {
            Some(retime) => ticks(retime.forward(seconds(new_duration)).max(0.0)),
            None => new_duration,
        };
        clipped.trim_end = (source - clipped.trim_start - span).max(MediaTime::ZERO);
    }
    if cut_start > MediaTime::ZERO {
        for channel in clipped.animations.values_mut() {
            channel.shift(MediaTime::ZERO - cut_start);
        }
        for word in &mut clipped.words {
            word.start = word.start - cut_start;
            word.end = word.end - cut_start;
        }
        clipped.words.retain(|word| word.end > MediaTime::ZERO && word.start < new_duration);
        if let Some(retime) = &mut clipped.retime {
            let shift = seconds(cut_start);
            for key in &mut retime.ramp {
                key.time -= shift;
            }
        }
    }
    clipped
}

/// The compound's children as ordinary clips on timeline time, clipped to
/// its visible window, one list per inner track (top to bottom). Ids are
/// `<compound id>~<child id>` so they never collide with real clips.
pub fn compound_children(compound: &Element) -> Vec<(CompoundTrack, Vec<Element>)> {
    let ElementKind::Compound { tracks } = &compound.kind else { return Vec::new() };
    let window_start = compound.trim_start;
    let window_end = compound.trim_start + compound.duration;
    tracks
        .iter()
        .map(|track| {
            let elements = track
                .elements
                .iter()
                .filter_map(|child| {
                    let start = child.start_time;
                    let end = child.end_time();
                    if end <= window_start || start >= window_end {
                        return None;
                    }
                    let cut_start = (window_start - start).max(MediaTime::ZERO);
                    let cut_end = (end - window_end).max(MediaTime::ZERO);
                    let mut placed = clip_element(child, cut_start, cut_end);
                    placed.start_time = compound.start_time + (start.max(window_start) - window_start);
                    placed.id = format!("{}~{}", compound.id, child.id);
                    placed.hidden = placed.hidden || compound.hidden;
                    if placed.duration <= MediaTime::ZERO {
                        return None;
                    }
                    Some(placed)
                })
                .collect();
            (track.clone(), elements)
        })
        .collect()
}

fn expand_tracks(tracks: &[Track], depth: usize) -> Vec<Track> {
    let mut out = Vec::with_capacity(tracks.len());
    for track in tracks {
        let compounds: Vec<&Element> =
            track.elements.iter().filter(|element| matches!(element.kind, ElementKind::Compound { .. })).collect();
        if compounds.is_empty() {
            out.push(track.clone());
            continue;
        }
        // The compounds' contents go just above the track they sit on.
        let mut synthetic = Vec::new();
        for compound in compounds {
            for (inner, elements) in compound_children(compound) {
                let mut child_track = Track::new(
                    inner.kind,
                    if inner.kind == TrackKind::Audio { Lane::Audio } else { Lane::Overlay },
                    inner.name.clone(),
                );
                child_track.id = format!("{}~{}~{}", track.id, compound.id, inner.id);
                child_track.muted = inner.muted || track.muted;
                child_track.hidden = inner.hidden || track.hidden;
                child_track.locked = track.locked;
                child_track.elements = elements;
                child_track.sort_elements();
                synthetic.push(child_track);
            }
        }
        let synthetic = if depth + 1 < MAX_NESTING && has_compounds(&synthetic) {
            expand_tracks(&synthetic, depth + 1)
        } else {
            synthetic
        };
        out.extend(synthetic);
        let mut host = track.clone();
        host.elements.retain(|element| !matches!(element.kind, ElementKind::Compound { .. }));
        out.push(host);
    }
    out
}

/// The project as renderers and the mixer see it: every compound swapped
/// for its clipped children. Borrows the project when it has none.
pub fn expand_compounds(project: &Project) -> Cow<'_, Project> {
    if !has_compounds(&project.tracks) {
        return Cow::Borrowed(project);
    }
    let mut expanded = project.clone();
    expanded.tracks = expand_tracks(&project.tracks, 0);
    Cow::Owned(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, MediaItem, MediaKind, Retime, TrackPlacement, apply, plan_audio};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn media(id: &str, kind: MediaKind) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            name: format!("{id}.mp4"),
            kind,
            path: std::path::PathBuf::from(format!("{id}.mp4")),
            duration: Some(seconds(10.0)),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            has_audio: true,
        }
    }

    fn insert(project: &mut Project, element: Element) -> String {
        apply(project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap().element_refs()[0]
            .element_id
            .clone()
    }

    #[test]
    fn clipping_keeps_source_frames_in_place() {
        let item = media("v", MediaKind::Video);
        let mut clip = Element::from_media(&item, seconds(0.0));
        clip.duration = seconds(4.0);
        clip.trim_start = seconds(1.0);
        let cut = clip_element(&clip, seconds(1.0), seconds(0.5));
        assert_eq!((cut.trim_start, cut.duration), (seconds(2.0), seconds(2.5)));
        // At 2x the first second of clip time is two seconds of source.
        clip.retime = Some(Retime::constant(2.0));
        let fast = clip_element(&clip, seconds(1.0), MediaTime::ZERO);
        assert_eq!(fast.trim_start, seconds(3.0));
        // Backwards, cutting the end moves the window's start on.
        clip.retime = Some(Retime { reverse: true, ..Retime::constant(1.0) });
        let back = clip_element(&clip, MediaTime::ZERO, seconds(1.0));
        assert_eq!(back.trim_start, seconds(2.0));
        let same_frame = |element: &Element, at: f64| element.source_time(seconds(at));
        let original = Element { retime: Some(Retime { reverse: true, ..Retime::constant(1.0) }), ..clip.clone() };
        let shifted = Element { start_time: seconds(0.5), ..clip_element(&original, seconds(0.5), seconds(1.0)) };
        assert_eq!(same_frame(&original, 1.0), same_frame(&shifted, 1.0));
    }

    #[test]
    fn compounds_round_trip_and_expand_like_their_children() {
        let mut project = Project::new("Compound");
        apply(&mut project, &Command::AddMedia(media("v", MediaKind::Video))).unwrap();
        apply(&mut project, &Command::AddMedia(media("a", MediaKind::Audio))).unwrap();
        let mut video = Element::from_media(project.media_item("v").unwrap(), seconds(2.0));
        video.duration = seconds(4.0);
        let video_id = insert(&mut project, video);
        let mut title = Element::text("Title", seconds(3.0));
        title.duration = seconds(2.0);
        let title_id = insert(&mut project, title);
        let mut music = Element::from_media(project.media_item("a").unwrap(), seconds(2.0));
        music.duration = seconds(4.0);
        let music_id = insert(&mut project, music);
        let before_frame = crate::plan_frame(&project, seconds(3.5));
        let before_audio = plan_audio(&project);

        let outcome = apply(
            &mut project,
            &Command::CreateCompound { element_ids: vec![video_id.clone(), title_id.clone(), music_id.clone()], compound_id: "c1".into() },
        )
        .unwrap();
        assert_eq!(outcome.element_refs()[0].element_id, "c1");
        let (_, compound) = project.element("c1").unwrap();
        assert_eq!((compound.start_time, compound.duration, compound.source_duration), (seconds(2.0), seconds(4.0), Some(seconds(4.0))));
        assert_eq!(compound_child_count(compound), 3);
        assert!(project.element(&video_id).is_none(), "the children left the timeline");

        // JSON keeps the web shape and reads back.
        let json = serde_json::to_value(compound).unwrap();
        assert_eq!(json["type"], "compound");
        assert_eq!(json["tracks"][0]["type"], "text");
        let back: Element = serde_json::from_value(json).unwrap();
        assert_eq!(&back, compound);

        // Expanded, the frame and the mix are what they were.
        let after_frame = crate::plan_frame(&project, seconds(3.5));
        assert_eq!(after_frame.items.len(), before_frame.items.len());
        let after_audio = plan_audio(&project);
        assert_eq!(after_audio.len(), before_audio.len());
        let starts = |clips: &[crate::AudioClipPlan]| clips.iter().map(|clip| clip.timeline_start).collect::<Vec<_>>();
        assert_eq!(starts(&after_audio), starts(&before_audio));
        assert!(after_audio.iter().any(|clip| clip.element_id == format!("c1~{music_id}")));

        // Trimming the compound's start cuts into its children.
        apply(&mut project, &Command::ResizeElement { element_id: "c1".into(), edge: crate::Edge::Start, time: seconds(4.0) }).unwrap();
        let expanded = expand_compounds(&project);
        let video_child = expanded.element(&format!("c1~{video_id}")).unwrap().1;
        assert_eq!((video_child.start_time, video_child.trim_start, video_child.duration), (seconds(4.0), seconds(2.0), seconds(2.0)));
        let title_child = expanded.element(&format!("c1~{title_id}")).unwrap().1;
        assert_eq!((title_child.start_time, title_child.duration), (seconds(4.0), seconds(1.0)));

        // Breaking it apart puts the visible pieces back as clips.
        let outcome = apply(&mut project, &Command::BreakApartCompound { element_id: "c1".into() }).unwrap();
        assert_eq!(outcome.element_refs().len(), 3);
        assert!(project.element("c1").is_none());
        let restored = project.element(&video_id).unwrap().1;
        assert_eq!((restored.start_time, restored.trim_start, restored.duration), (seconds(4.0), seconds(2.0), seconds(2.0)));
        apply(&mut project, &Command::DeleteElements(vec![music_id])).unwrap();
    }

    #[test]
    fn groups_select_and_duplicate_together() {
        let mut project = Project::new("Groups");
        let a = insert(&mut project, Element::text("A", seconds(0.0)));
        let b = insert(&mut project, Element::text("B", seconds(6.0)));
        let c = insert(&mut project, Element::text("C", seconds(12.0)));
        apply(&mut project, &Command::GroupElements { element_ids: vec![a.clone(), b.clone()], group_id: "g1".into() }).unwrap();
        assert_eq!(crate::group_members(&project, &a), vec![a.clone(), b.clone()]);
        assert_eq!(crate::group_members(&project, &c), vec![c.clone()]);
        assert!(apply(&mut project, &Command::GroupElements { element_ids: vec![c.clone()], group_id: "g2".into() }).is_err());

        let outcome = apply(&mut project, &Command::DuplicateElements(vec![a.clone(), b.clone()])).unwrap();
        let copies: Vec<String> = outcome.element_refs().iter().map(|r| r.element_id.clone()).collect();
        let copy_group = project.element(&copies[0]).unwrap().1.group_id.clone();
        assert!(copy_group.is_some() && copy_group.as_deref() != Some("g1"), "copies form their own group");
        assert_eq!(project.element(&copies[1]).unwrap().1.group_id, copy_group);

        apply(&mut project, &Command::UngroupElements(vec![b.clone()])).unwrap();
        assert!(project.element(&a).unwrap().1.group_id.is_none(), "ungrouping one member ungroups the group");
    }
}

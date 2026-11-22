//! Freeze frame: a video clip holds one frame for a while. The clip is
//! split at the frozen moment, a still of that frame goes in between as an
//! image clip with the clip's look, and everything that starts at or after
//! that moment on unlocked tracks moves along by the freeze length, so
//! separated audio and captions stay in step. The web editor's
//! `src/freeze/freeze.ts` does the same. See [`crate::Command::FreezeFrame`].

use time::MediaTime;

use crate::{Element, ElementKind, resolve_params};

/// The freeze length the toolbar uses.
pub const FREEZE_DEFAULT_SECONDS: f64 = 3.0;
pub const FREEZE_MIN_SECONDS: f64 = 0.1;
pub const FREEZE_MAX_SECONDS: f64 = 60.0;

/// The still clip that stands in for `element` at timeline time `at` for
/// `length`, showing `media_id` (the frame as a picture): the clip's
/// params with animated values fixed at that moment, its effects and masks.
pub fn freeze_still(element: &Element, at: MediaTime, length: MediaTime, media_id: &str) -> Element {
    let local = (at - element.start_time).max(MediaTime::ZERO).min(element.duration);
    let mut params = resolve_params(&element.params, &element.animations, local);
    for key in ["volume", "muted"] {
        params.remove(key);
    }
    Element {
        id: crate::new_id(),
        name: format!("{} (freeze)", element.name),
        kind: ElementKind::Image { media_id: media_id.to_owned() },
        start_time: at,
        duration: length,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: None,
        hidden: false,
        params,
        animations: Default::default(),
        effects: element.effects.clone(),
        masks: element.masks.clone(),
        transition: None,
        transition_in: None,
        cutout: None,
        words: Default::default(),
        retime: None,
        group_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Editor, MediaItem, MediaKind, Project, Segment, TrackPlacement};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn media(id: &str, kind: MediaKind, duration: Option<f64>) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            path: format!("{id}.bin").into(),
            width: Some(1920),
            height: Some(1080),
            duration: duration.map(seconds),
            fps: Some(30.0),
            has_audio: kind != MediaKind::Image,
        }
    }

    #[test]
    fn freezing_splits_the_clip_and_moves_later_clips_along() {
        let mut editor = Editor::new(Project::new("Freeze"));
        editor.execute(&Command::AddMedia(media("clip", MediaKind::Video, Some(20.0)))).unwrap();
        editor.execute(&Command::AddMedia(media("voice", MediaKind::Audio, Some(5.0)))).unwrap();
        editor.execute(&Command::AddMedia(media("music", MediaKind::Audio, Some(40.0)))).unwrap();
        editor.execute(&Command::AddMedia(media("still", MediaKind::Image, None))).unwrap();
        let mut video = Element::from_media(editor.project().media_item("clip").unwrap(), seconds(10.0));
        video.trim_start = seconds(5.0);
        video.animations.entry("opacity".to_owned()).or_default().upsert(MediaTime::ZERO, 0.0, Some(Segment::Linear));
        video.animations.get_mut("opacity").unwrap().upsert(seconds(4.0), 1.0, Some(Segment::Linear));
        let video_id = video.id.clone();
        editor.execute(&Command::InsertElement { element: video, placement: TrackPlacement::Auto }).unwrap();
        let voice = Element::from_media(editor.project().media_item("voice").unwrap(), seconds(12.0));
        let voice_id = voice.id.clone();
        editor.execute(&Command::InsertElement { element: voice, placement: TrackPlacement::Auto }).unwrap();
        let music = Element::from_media(editor.project().media_item("music").unwrap(), seconds(0.0));
        let music_id = music.id.clone();
        editor.execute(&Command::InsertElement { element: music, placement: TrackPlacement::Auto }).unwrap();

        let outcome = editor.execute(&Command::FreezeFrame { element_id: video_id.clone(), time: seconds(12.0), length: seconds(3.0), media_id: "still".to_owned() })
            .unwrap();
        let still_id = outcome.element_refs()[0].element_id.clone();
        let project = editor.project();
        let at = |id: &str| {
            let (track, index) = project.locate(id).unwrap();
            let element = &project.tracks[track].elements[index];
            (element.start_time.to_seconds_f64(), element.duration.to_seconds_f64())
        };
        assert_eq!(at(&video_id), (10.0, 2.0));
        assert_eq!(at(&still_id), (12.0, 3.0));
        assert_eq!(at(&voice_id), (15.0, 5.0), "separated audio after the playhead keeps in step");
        assert_eq!(at(&music_id), (0.0, 40.0), "music under the playhead plays on");
        let (track, index) = project.locate(&still_id).unwrap();
        let still = &project.tracks[track].elements[index];
        assert!(matches!(&still.kind, ElementKind::Image { media_id } if media_id == "still"));
        assert!((still.params.number("opacity", -1.0) - 0.5).abs() < 1e-6, "animated opacity fixed at the frozen moment");
        assert!(still.animations.is_empty());
        let right = project.tracks[track].elements.iter().find(|element| element.start_time == seconds(15.0) && element.id != voice_id).unwrap();
        assert_eq!(right.trim_start, seconds(7.0), "the rest of the clip carries on from the frozen frame");

        editor.undo().unwrap();
        assert_eq!(editor.project().tracks.iter().map(|track| track.elements.len()).sum::<usize>(), 3);
        assert!(editor.execute(&Command::FreezeFrame { element_id: voice_id, time: seconds(13.0), length: seconds(3.0), media_id: "still".to_owned() })
            .is_err(), "only video clips freeze");
        assert!(editor.execute(&Command::FreezeFrame { element_id: video_id, time: seconds(31.0), length: seconds(3.0), media_id: "still".to_owned() })
            .is_err(), "the time must be on the clip");
    }
}

//! Which pieces of which files are audible, and when.

use std::path::PathBuf;

use time::MediaTime;

use crate::animation::Channel;
use crate::document::{ElementKind, MediaKind, Project};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioClipPlan {
    pub element_id: String,
    pub media_id: String,
    pub path: PathBuf,
    /// Where on the timeline the clip starts.
    pub timeline_start: MediaTime,
    /// The part of the source that plays.
    pub source_start: MediaTime,
    pub source_end: MediaTime,
    /// Gain in decibels; 0 is unchanged. The value when the volume has no keys.
    pub volume_db: f64,
    /// Keyframed volume in decibels, relative to the clip's start; empty when
    /// the volume is static.
    pub volume: Channel,
}

impl AudioClipPlan {
    pub fn timeline_end(&self) -> MediaTime {
        self.timeline_start + (self.source_end - self.source_start)
    }

    /// The static gain (the volume's keys, if any, are ignored).
    pub fn gain(&self) -> f64 {
        db_to_gain(self.volume_db)
    }

    pub fn is_animated(&self) -> bool {
        !self.volume.is_empty()
    }

    /// Linear gain at a timeline time, following the volume keys.
    pub fn gain_at(&self, timeline_time: MediaTime) -> f64 {
        let db = self
            .volume
            .value_at(timeline_time - self.timeline_start)
            .unwrap_or(self.volume_db);
        db_to_gain(db)
    }

    /// `gain_at` for a time in seconds.
    pub fn gain_at_seconds(&self, timeline_seconds: f64) -> f64 {
        match MediaTime::from_seconds_f64(timeline_seconds) {
            Some(time) => self.gain_at(time),
            None => self.gain(),
        }
    }
}

fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Every audible clip in the project, in timeline order. Muted tracks and
/// elements are left out, as are video clips whose source has no audio.
pub fn plan_audio(project: &Project) -> Vec<AudioClipPlan> {
    let mut clips = Vec::new();
    for track in &project.tracks {
        if track.muted {
            continue;
        }
        for element in &track.elements {
            if element.hidden || element.params.bool("muted", false) {
                continue;
            }
            let media_id = match &element.kind {
                ElementKind::Video { media_id } | ElementKind::Audio { media_id } => media_id,
                ElementKind::Image { .. } | ElementKind::Text => continue,
            };
            let Some(media) = project.media_item(media_id) else {
                continue;
            };
            if media.kind == MediaKind::Video && !media.has_audio {
                continue;
            }
            clips.push(AudioClipPlan {
                element_id: element.id.clone(),
                media_id: media_id.clone(),
                path: media.path.clone(),
                timeline_start: element.start_time,
                source_start: element.trim_start,
                source_end: element.trim_start + element.duration,
                volume_db: element.params.number("volume", 0.0),
                volume: element.animations.get("volume").cloned().unwrap_or_default(),
            });
        }
    }
    clips.sort_by_key(|clip| clip.timeline_start);
    clips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TrackPlacement, apply};
    use crate::document::{Element, MediaItem};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn plans_audible_clips_only() {
        let mut project = Project::new("Audio");
        for (id, kind, has_audio) in [
            ("v", MediaKind::Video, true),
            ("silent", MediaKind::Video, false),
            ("a", MediaKind::Audio, true),
        ] {
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.to_owned(),
                    name: id.to_owned(),
                    kind,
                    path: format!("{id}.bin").into(),
                    duration: Some(seconds(4.0)),
                    width: None,
                    height: None,
                    fps: None,
                    has_audio,
                }),
            )
            .unwrap();
        }
        for id in ["v", "silent", "a"] {
            let mut element = Element::from_media(project.media_item(id).unwrap(), seconds(1.0));
            if id == "a" {
                element.params.set("volume", -6.0);
                element.trim_start = seconds(1.0);
                element.duration = seconds(2.0);
            }
            apply(
                &mut project,
                &Command::InsertElement {
                    element,
                    placement: TrackPlacement::Auto,
                },
            )
            .unwrap();
        }
        let clips = plan_audio(&project);
        assert_eq!(clips.len(), 2);
        let audio = clips.iter().find(|clip| clip.media_id == "a").unwrap();
        assert_eq!(audio.source_start, seconds(1.0));
        assert_eq!(audio.source_end, seconds(3.0));
        assert_eq!(audio.timeline_end(), seconds(3.0));
        assert!((audio.gain() - 0.501).abs() < 0.001);
        assert!(!audio.is_animated());
        assert!((audio.gain_at(seconds(2.0)) - audio.gain()).abs() < 1e-9);
    }

    #[test]
    fn follows_volume_keyframes() {
        let mut project = Project::new("Fade");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "a".to_owned(),
                name: "a".to_owned(),
                kind: MediaKind::Audio,
                path: "a.bin".into(),
                duration: Some(seconds(4.0)),
                width: None,
                height: None,
                fps: None,
                has_audio: true,
            }),
        )
        .unwrap();
        let element = Element::from_media(project.media_item("a").unwrap(), seconds(1.0));
        let element_id = element.id.clone();
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        for (time, value) in [(1.0, -20.0), (3.0, 0.0)] {
            apply(
                &mut project,
                &Command::SetKeyframe {
                    element_id: element_id.clone(),
                    path: "volume".to_owned(),
                    time: seconds(time),
                    value,
                    segment: None,
                },
            )
            .unwrap();
        }
        let clips = plan_audio(&project);
        let clip = &clips[0];
        assert!(clip.is_animated());
        assert!((clip.gain_at(seconds(0.5)) - 0.1).abs() < 1e-9, "held before the first key");
        assert!((clip.gain_at(seconds(2.0)) - 10f64.powf(-0.5)).abs() < 1e-9, "linear in dB");
        assert!((clip.gain_at_seconds(4.0) - 1.0).abs() < 1e-9, "held after the last key");
        assert!((clip.gain() - 1.0).abs() < 1e-9, "the static volume is unchanged");
    }
}

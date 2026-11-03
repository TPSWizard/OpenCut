//! Which pieces of which files are audible, and when.

use std::path::PathBuf;

use time::MediaTime;

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
    /// Gain in decibels; 0 is unchanged.
    pub volume_db: f64,
}

impl AudioClipPlan {
    pub fn timeline_end(&self) -> MediaTime {
        self.timeline_start + (self.source_end - self.source_start)
    }

    pub fn gain(&self) -> f64 {
        10f64.powf(self.volume_db / 20.0)
    }
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
    }
}

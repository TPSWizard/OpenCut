//! Effects on a clip's faces: the clip keeps its face track
//! (`facetrack::FACE_TRACK_PARAM`) and an effect that reads it is turned on
//! (added at its defaults when the clip has none), in one undo step:
//! Retouch set to "faces only", or Face blur. The web editor builds the
//! same changes (`apps/web/src/retouch/faces.ts`).

use facetrack::{FACE_TRACK_PARAM, FaceTrack};

use crate::commands::Command;
use crate::document::{ElementKind, Project};
use crate::params::Params;
use crate::{Effect, ElementPatch};

/// Keeps `track` on the clip and turns on its `kind` effect with `set`.
fn face_effect_commands(
    project: &Project,
    element_id: &str,
    track: &FaceTrack,
    kind: &str,
    set: &[(&str, &str)],
) -> Result<Vec<Command>, String> {
    let (_, element) = project.element(element_id).ok_or("The clip is no longer on the timeline")?;
    if !matches!(element.kind, ElementKind::Video { .. } | ElementKind::Image { .. }) {
        return Err("Faces can be found in video and picture clips only".to_owned());
    }
    let mut params = Params::new();
    params.set(FACE_TRACK_PARAM, track.encode());
    let mut commands =
        vec![Command::UpdateElement { element_id: element_id.to_owned(), patch: ElementPatch { params: Some(params), ..Default::default() } }];
    let mut values = Params::new();
    for (key, value) in set {
        values.set(*key, *value);
    }
    match element.effects.iter().find(|effect| effect.kind == kind) {
        Some(effect) => commands.push(Command::UpdateEffect {
            element_id: element_id.to_owned(),
            effect_id: effect.id.clone(),
            params: (!set.is_empty()).then_some(values),
            enabled: Some(true),
        }),
        None => {
            let mut effect = Effect::from_catalogue(kind).ok_or_else(|| format!("The {kind} effect is missing"))?;
            for (key, value) in set {
                effect.params.set(*key, *value);
            }
            commands.push(Command::AddEffect { element_id: element_id.to_owned(), effect });
        }
    }
    Ok(commands)
}

/// Face-only retouch: the track, and Retouch with Area "faces only".
pub fn face_retouch_commands(project: &Project, element_id: &str, track: &FaceTrack) -> Result<Vec<Command>, String> {
    face_effect_commands(project, element_id, track, "retouch", &[("area", "faces")])
}

/// Face blur: the track, and the Face blur effect (kept as it is when the
/// clip already has one, else added at its defaults).
pub fn face_blur_commands(project: &Project, element_id: &str, track: &FaceTrack) -> Result<Vec<Command>, String> {
    face_effect_commands(project, element_id, track, "face-blur", &[])
}

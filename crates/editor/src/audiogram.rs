//! Audiograms (row 100) on the timeline: a graphic element (definition
//! [`AUDIOGRAM_DEFINITION`]) over the whole canvas whose `levels` param
//! holds a clip's sound analysed once per frame by the shared `audiogram`
//! crate, drawn as bars, mirrored bars, a line or a circle. Made from a
//! video or audio clip with sound; it starts where the clip does and lasts
//! as long (at most [`audiogram::MAX_SECONDS`]).

use crate::{Element, ElementKind, Params};
use time::MediaTime;

pub use audiogram::{Levels as AudiogramLevels, Look as AudiogramLook, Style as AudiogramStyle};

/// The graphic definition id of an audiogram element.
pub const AUDIOGRAM_DEFINITION: &str = audiogram::DEFINITION;

pub fn is_audiogram(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Graphic { definition_id } if definition_id == AUDIOGRAM_DEFINITION)
}

/// A clip an audiogram can be made from: video or audio (it must also
/// have sound, which only decoding can tell).
pub fn can_make_audiogram(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Video { .. } | ElementKind::Audio { .. })
}

/// How many level frames an audiogram of `element` has.
pub fn audiogram_frame_count(element: &Element) -> usize {
    let seconds = element.duration.to_seconds_f64().min(audiogram::MAX_SECONDS);
    ((seconds * audiogram::FPS).ceil() as usize).max(1)
}

/// The source second each level frame of an audiogram of `element` looks
/// at (following any speed change).
pub fn audiogram_source_times(element: &Element) -> Vec<f64> {
    (0..audiogram_frame_count(element))
        .map(|frame| {
            let local = MediaTime::from_seconds_f64(frame as f64 / audiogram::FPS).unwrap_or(MediaTime::ZERO);
            let at = (element.start_time + local).min(element.end_time() - MediaTime::ONE_TICK).max(element.start_time);
            element.source_time(at).to_seconds_f64()
        })
        .collect()
}

/// An audiogram element for `source` with its analysed `levels`: from the
/// clip's start, as long as the levels last, in the default look.
pub fn audiogram_element(source: &Element, levels: &AudiogramLevels) -> Element {
    let mut element = Element::sticker("", "Audiogram", source.start_time);
    element.kind = ElementKind::Graphic { definition_id: AUDIOGRAM_DEFINITION.to_owned() };
    let seconds = (levels.seconds()).min(source.duration.to_seconds_f64()).max(1.0 / audiogram::FPS);
    element.duration = MediaTime::from_seconds_f64(seconds).unwrap_or(source.duration);
    for (key, value) in audiogram::default_params() {
        match value {
            serde_json::Value::String(text) => element.params.set(key, text),
            serde_json::Value::Number(number) => element.params.set(key, number.as_f64().unwrap_or(0.0)),
            _ => &mut element.params,
        };
    }
    element.params.set("levels", levels.encode());
    element.params.set("sourceName", source.name.clone());
    element
}

/// The audiogram's look from its params.
pub fn read_audiogram_look(params: &Params) -> AudiogramLook {
    AudiogramLook::from_lookup(
        |key| match params.get(key)? {
            crate::ParamValue::Text(text) => Some(text.clone()),
            _ => None,
        },
        |key| params.get(key).and_then(crate::ParamValue::as_number),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Project, TrackPlacement, apply, plan_frame, LayerSource};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn makes_an_element_that_plans_as_an_audiogram() {
        let mut clip = Element::text("clip", seconds(2.0));
        clip.duration = seconds(1.0);
        clip.trim_start = seconds(5.0);
        let times = audiogram_source_times(&clip);
        assert_eq!(times.len(), 30);
        assert!((times[0] - 5.0).abs() < 1e-9 && (times[29] - (5.0 + 29.0 / 30.0)).abs() < 1e-6);
        let levels = AudiogramLevels { fps: 30.0, bands: 2, data: vec![255; 60] };
        let element = audiogram_element(&clip, &levels);
        assert!(is_audiogram(&element));
        assert_eq!(element.start_time, seconds(2.0));
        assert_eq!(element.duration, seconds(1.0));
        assert_eq!(read_audiogram_look(&element.params), AudiogramLook::default());

        let mut project = Project::new("Audiogram");
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        let plan = plan_frame(&project, seconds(2.5));
        let found = plan.layers().iter().find_map(|layer| match &layer.source {
            LayerSource::Audiogram { seconds, look, .. } => Some((*seconds, look.clone())),
            _ => None,
        });
        let (at, look) = found.expect("planned");
        assert!((at - 0.5).abs() < 1e-9);
        assert_eq!(look.style, AudiogramStyle::Mirror);
    }
}

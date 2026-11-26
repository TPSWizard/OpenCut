//! The text template library (row 98) as clips: the shared `titles`
//! crate's templates, placed for the canvas, become grouped text clips.

use time::MediaTime;

use crate::{CanvasSize, Element, ParamValue};
pub use titles::{
    PlacedTextItem, TextTemplate, TextTemplateCategory, TextTemplateItem, text_template, text_template_categories,
    text_templates,
};

/// The template's clips placed for `canvas` (see `titles::place_text_template`).
pub fn place_text_template(template: &TextTemplate, canvas: CanvasSize) -> Vec<PlacedTextItem> {
    titles::place_text_template(template, canvas.width, canvas.height)
}

/// How much a template's font sizes shrink on this canvas.
pub fn text_template_font_scale(canvas: CanvasSize) -> f64 {
    titles::text_template_font_scale(canvas.width, canvas.height)
}

/// The template as text clips starting at `start` on `canvas`, grouped
/// when there is more than one (insert them together as one undo step).
pub fn text_template_elements(template: &TextTemplate, canvas: CanvasSize, start: MediaTime) -> Vec<Element> {
    let placed = place_text_template(template, canvas);
    let group = (placed.len() > 1).then(crate::new_id);
    placed
        .into_iter()
        .map(|item| {
            let at = start + MediaTime::from_seconds_f64(item.start).unwrap_or(MediaTime::ZERO);
            let mut element = Element::text("", at);
            element.name = item.name;
            element.duration = MediaTime::from_seconds_f64(item.duration).unwrap_or(element.duration);
            for (key, value) in item.params {
                element.params.set(key, ParamValue::from(value));
            }
            element.group_id = group.clone();
            element
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_use_the_editors_animation_presets() {
        for template in text_templates() {
            for item in &template.items {
                for (key, known) in [
                    ("animIn", crate::ANIMATION_IN_KINDS),
                    ("animOut", crate::ANIMATION_OUT_KINDS),
                    ("animLoop", crate::ANIMATION_LOOP_KINDS),
                ] {
                    if let Some(name) = item.params.get(key).and_then(|value| value.as_str()) {
                        assert!(known.contains(&name), "{} {key} {name}", template.id);
                    }
                }
            }
        }
    }

    #[test]
    fn a_template_becomes_grouped_text_clips() {
        let template = text_template("name-and-role").unwrap();
        let elements = text_template_elements(template, CanvasSize { width: 1920, height: 1080 }, MediaTime::from_seconds_f64(2.0).unwrap());
        assert_eq!(elements.len(), 2);
        assert!(elements[0].group_id.is_some() && elements[0].group_id == elements[1].group_id);
        assert_eq!(elements[1].start_time, MediaTime::from_seconds_f64(2.2).unwrap());
        assert_eq!(elements[0].params.text("content", ""), "Alex Morgan");
        assert!(elements[0].params.bool("background.enabled", false));
        let credits = text_template_elements(text_template("end-credits").unwrap(), CanvasSize { width: 1080, height: 1920 }, MediaTime::ZERO);
        assert_eq!(credits.len(), 1);
        assert!(credits[0].group_id.is_none());
        assert!(crate::read_credit_roll(&credits[0].params).is_some());
    }

    #[test]
    fn inserting_a_template_is_one_undo_step_and_stays_grouped() {
        let mut editor = crate::Editor::new(crate::Project::new("Templates"));
        let elements = text_template_elements(text_template("subscribe-button").unwrap(), CanvasSize { width: 1920, height: 1080 }, MediaTime::ZERO);
        let commands = elements
            .into_iter()
            .map(|element| crate::Command::InsertElement { element, placement: crate::TrackPlacement::Auto })
            .collect();
        editor.execute(&crate::Command::Batch(commands)).unwrap();
        let texts: Vec<&Element> = editor.project().tracks.iter().flat_map(|track| &track.elements).collect();
        assert_eq!(texts.len(), 2);
        assert!(texts[0].group_id.is_some() && texts[0].group_id == texts[1].group_id);
        editor.undo().unwrap();
        assert_eq!(editor.project().tracks.iter().map(|track| track.elements.len()).sum::<usize>(), 0);
    }
}

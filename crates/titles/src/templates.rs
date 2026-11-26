//! The text template library (row 98): titles, lower thirds, subscribe
//! cards, end credits and a crawl, our own designs in
//! `templates/text-templates.json`. A template is one or more text clips
//! with their look (background box, shadow), animation presets (row 62)
//! and, for credits, a roll (`credit_roll`). Placing one makes grouped
//! text clips sized and placed for the canvas: positions are shares of the
//! canvas from its centre, and font sizes (in the editor's units, which
//! follow the canvas height) shrink on canvases narrower than 16:9 so a
//! line still fits. The editor turns placed items into clips
//! (`editor::text_template_elements`); the web editor reads the same file
//! and placement through WASM.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// The library as written, for front ends that list it.
pub const LIBRARY_JSON: &str = include_str!("../text-templates.json");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TextTemplateCategory {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TextTemplate {
    pub id: String,
    pub label: String,
    pub category: String,
    pub description: String,
    /// Seconds.
    pub duration: f64,
    pub items: Vec<TextTemplateItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplateItem {
    pub name: String,
    pub content: String,
    /// Shares of the canvas width and height from its centre.
    pub x: f64,
    pub y: f64,
    /// In the editor's font units at 16:9 (the canvas height / 90 per unit).
    pub font_size: f64,
    /// Seconds after the template's start.
    #[serde(default)]
    pub start: f64,
    /// Seconds; the rest of the template when absent.
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Library {
    categories: Vec<TextTemplateCategory>,
    templates: Vec<TextTemplate>,
}

fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| serde_json::from_str(LIBRARY_JSON).expect("text-templates.json parses"))
}

pub fn text_template_categories() -> &'static [TextTemplateCategory] {
    &library().categories
}

pub fn text_templates() -> &'static [TextTemplate] {
    &library().templates
}

pub fn text_template(id: &str) -> Option<&'static TextTemplate> {
    text_templates().iter().find(|template| template.id == id)
}

/// How much a template's font sizes shrink on this canvas: 1 at 16:9 or
/// wider, the square root of the width ratio narrower (0.75 at 1:1, about
/// 0.56 at 9:16), so lines still fit across.
pub fn text_template_font_scale(canvas_width: u32, canvas_height: u32) -> f64 {
    let aspect = canvas_width.max(1) as f64 / canvas_height.max(1) as f64;
    (aspect / (16.0 / 9.0)).sqrt().min(1.0)
}

/// One text clip of a placed template: when it starts (seconds after the
/// template's start), how long it lasts and its complete params (content,
/// look, position and animations), as both editors store them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlacedTextItem {
    pub name: String,
    pub start: f64,
    pub duration: f64,
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// The template's clips for `canvas`: each item's own params over the
/// text defaults, with its content, position and font size.
pub fn place_text_template(template: &TextTemplate, canvas_width: u32, canvas_height: u32) -> Vec<PlacedTextItem> {
    let font_scale = text_template_font_scale(canvas_width, canvas_height);
    template
        .items
        .iter()
        .map(|item| {
            let start = item.start.clamp(0.0, template.duration);
            let duration = item.duration.unwrap_or(template.duration - start).max(0.1);
            let mut params = item.params.clone();
            let number = |value: f64| serde_json::Value::from((value * 1000.0).round() / 1000.0);
            params.insert("content".to_owned(), serde_json::Value::from(item.content.clone()));
            params.insert("fontSize".to_owned(), number(item.font_size * font_scale));
            params.insert("transform.positionX".to_owned(), number(item.x * canvas_width as f64));
            params.insert("transform.positionY".to_owned(), number(item.y * canvas_height as f64));
            PlacedTextItem { name: item.name.clone(), start, duration, params }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Fixture {
        placements: Vec<PlacementCase>,
    }

    #[derive(Deserialize)]
    struct PlacementCase {
        template: String,
        canvas: [u32; 2],
        items: Vec<ExpectedItem>,
    }

    #[derive(Deserialize)]
    struct ExpectedItem {
        name: String,
        start: f64,
        duration: f64,
        #[serde(rename = "fontSize")]
        font_size: f64,
        x: f64,
        y: f64,
    }

    #[test]
    fn placements_match_the_reference() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../../fixtures/text-templates/cases.json")).unwrap();
        assert!(fixture.placements.len() >= text_templates().len());
        for case in fixture.placements {
            let template = text_template(&case.template).unwrap();
            let placed = place_text_template(template, case.canvas[0], case.canvas[1]);
            assert_eq!(placed.len(), case.items.len(), "{}", case.template);
            for (got, want) in placed.iter().zip(&case.items) {
                let number = |key: &str| got.params[key].as_f64().unwrap();
                assert_eq!(got.name, want.name);
                for (label, a, b) in [
                    ("start", got.start, want.start),
                    ("duration", got.duration, want.duration),
                    ("fontSize", number("fontSize"), want.font_size),
                    ("x", number("transform.positionX"), want.x),
                    ("y", number("transform.positionY"), want.y),
                ] {
                    assert!((a - b).abs() < 1e-9, "{} {} {label}: {a} vs {b}", case.template, want.name);
                }
            }
        }
    }

    #[test]
    fn every_template_is_well_formed() {
        let categories: Vec<&str> = text_template_categories().iter().map(|c| c.id.as_str()).collect();
        assert!(text_templates().len() >= 9);
        for template in text_templates() {
            assert!(categories.contains(&template.category.as_str()), "{}", template.id);
            assert!(!template.items.is_empty() && template.duration > 0.0);
            for item in &template.items {
                assert!(item.x.abs() <= 0.5 && item.y.abs() <= 0.5, "{} is on the canvas", item.name);
                if let Some(roll) = item.params.get("roll").and_then(|value| value.as_str()) {
                    assert!(crate::RollDirection::parse(roll).is_some(), "{} roll {roll}", template.id);
                }
                for (key, value) in &item.params {
                    if let Some(kind) = key.strip_prefix("anim").filter(|rest| matches!(*rest, "In" | "Out" | "Loop")) {
                        let name = value.as_str().unwrap();
                        // The editor's presets; `editor` checks against its own lists too.
                        let known: &[&str] = match kind {
                            "In" => &["fade", "pop", "grow", "shrink", "slideUp", "slideDown", "slideLeft", "slideRight", "bounce", "spin", "typewriter", "wordByWord"],
                            "Out" => &["fade", "pop", "grow", "shrink", "slideUp", "slideDown", "slideLeft", "slideRight", "spin"],
                            _ => &["pulse", "shake", "wobble", "float", "kenBurns"],
                        };
                        assert!(known.contains(&name), "{} {key} {name}", template.id);
                    }
                }
            }
        }
    }
}

//! Builds the starter templates both editors offer: one project bundle per
//! template in `apps/web/public/templates/` plus `index.json` describing
//! them. The templates are ConzaCut's own designs (text, colours and
//! keyframes only, no media) using OFL fonts from Fontsource, so both
//! editors draw them.
//!
//! cargo run -p editor --example build_templates

use std::path::PathBuf;

use editor::{Background, CanvasSize, Command, Element, Project, TrackPlacement, apply, export_bundle};
use serde_json::json;
use time::{FrameRate, MediaTime};

fn s(seconds: f64) -> MediaTime {
    MediaTime::from_seconds_f64(seconds).expect("time")
}

struct Text<'a> {
    content: &'a str,
    at: f64,
    duration: f64,
    font: &'a str,
    size: f64,
    color: &'a str,
    bold: bool,
    italic: bool,
    align: &'a str,
    x: f64,
    y: f64,
    scale: f64,
    box_color: Option<&'a str>,
    /// Fades and grows in over this many seconds (0 for none).
    pop: f64,
    /// Slides in from this many pixels below (0 for none).
    rise: f64,
}

impl Default for Text<'_> {
    fn default() -> Self {
        Self {
            content: "",
            at: 0.0,
            duration: 5.0,
            font: "Inter",
            size: 6.0,
            color: "#ffffff",
            bold: false,
            italic: false,
            align: "center",
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            box_color: None,
            pop: 0.0,
            rise: 0.0,
        }
    }
}

fn add_text(project: &mut Project, name: &str, text: Text) {
    let mut element = Element::text(text.content, s(text.at));
    element.name = name.to_owned();
    element.duration = s(text.duration);
    element
        .params
        .set("fontFamily", text.font)
        .set("fontSize", text.size)
        .set("color", text.color)
        .set("textAlign", text.align)
        .set("fontWeight", if text.bold { "bold" } else { "normal" })
        .set("fontStyle", if text.italic { "italic" } else { "normal" })
        .set("transform.positionX", text.x)
        .set("transform.positionY", text.y)
        .set("transform.scaleX", text.scale)
        .set("transform.scaleY", text.scale);
    if let Some(color) = text.box_color {
        element
            .params
            .set("background.enabled", true)
            .set("background.color", color)
            .set("background.cornerRadius", 18.0)
            .set("background.paddingX", 34.0)
            .set("background.paddingY", 20.0);
    }
    if text.pop > 0.0 {
        let opacity = element.animations.entry("opacity".to_owned()).or_default();
        opacity.upsert(s(0.0), 0.0, None);
        opacity.upsert(s(text.pop), 1.0, None);
        for channel in ["transform.scaleX", "transform.scaleY"] {
            let scale = element.animations.entry(channel.to_owned()).or_default();
            scale.upsert(s(0.0), text.scale * 0.85, None);
            scale.upsert(s(text.pop), text.scale, None);
        }
    }
    if text.rise != 0.0 {
        let opacity = element.animations.entry("opacity".to_owned()).or_default();
        opacity.upsert(s(0.0), 0.0, None);
        opacity.upsert(s(0.4), 1.0, None);
        let y = element.animations.entry("transform.positionY".to_owned()).or_default();
        y.upsert(s(0.0), text.y + text.rise, None);
        y.upsert(s(0.4), text.y, None);
    }
    apply(project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).expect("insert text");
}

struct Template {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    project: Project,
    preview: serde_json::Value,
}

fn base(name: &str, width: u32, height: u32, background: &str) -> Project {
    let mut project = Project::new(name);
    project.settings.fps = FrameRate::FPS_30;
    project.settings.canvas = CanvasSize { width, height };
    project.settings.background = Background::Color { color: background.to_owned() };
    project
}

fn templates() -> Vec<Template> {
    let mut out = Vec::new();

    let mut hook = base("Vertical hook", 1080, 1920, "#111827");
    add_text(&mut hook, "Hook", Text {
        content: "Your hook\ngoes here",
        size: 6.5,
        color: "#111827",
        bold: true,
        y: -420.0,
        box_color: Some("#facc15"),
        pop: 0.35,
        duration: 6.0,
        ..Text::default()
    });
    add_text(&mut hook, "Promise", Text {
        content: "Say what they get\nin one line",
        at: 0.4,
        duration: 5.6,
        size: 5.0,
        scale: 0.7,
        y: -40.0,
        rise: 60.0,
        ..Text::default()
    });
    add_text(&mut hook, "Call to action", Text {
        content: "Follow for part 2",
        at: 4.0,
        duration: 2.0,
        size: 5.0,
        scale: 0.6,
        color: "#111827",
        bold: true,
        y: 640.0,
        box_color: Some("#ffffff"),
        pop: 0.3,
        ..Text::default()
    });
    out.push(Template {
        id: "vertical-hook",
        name: "Vertical hook",
        description: "A bold opening line, a one-line promise and a call to action for Shorts, Reels and TikTok.",
        category: "Social",
        project: hook,
        preview: json!({"background": "#111827", "title": "Your hook goes here", "color": "#111827", "box": "#facc15", "font": "Inter"}),
    });

    let mut title = base("Landscape title", 1920, 1080, "#0f172a");
    add_text(&mut title, "Title", Text {
        content: "Episode title",
        size: 9.0,
        bold: true,
        y: -50.0,
        rise: 50.0,
        duration: 4.0,
        ..Text::default()
    });
    add_text(&mut title, "Subtitle", Text {
        content: "Season 1 · Episode 4",
        at: 0.3,
        duration: 3.7,
        size: 5.0,
        scale: 0.8,
        color: "#94a3b8",
        y: 90.0,
        rise: 40.0,
        ..Text::default()
    });
    out.push(Template {
        id: "landscape-title",
        name: "Landscape title",
        description: "A 16:9 title card with a subtitle that slides in, for YouTube intros and chapters.",
        category: "Titles",
        project: title,
        preview: json!({"background": "#0f172a", "title": "Episode title", "color": "#ffffff", "font": "Inter"}),
    });

    let mut quote = base("Quote card", 1080, 1080, "#f5f0e6");
    add_text(&mut quote, "Quote", Text {
        content: "“Make the thing\nyou wish existed.”",
        font: "Playfair Display",
        size: 6.5,
        color: "#1f2937",
        italic: true,
        y: -40.0,
        pop: 0.6,
        ..Text::default()
    });
    add_text(&mut quote, "Attribution", Text {
        content: "— Your name",
        at: 0.8,
        duration: 4.2,
        size: 5.0,
        scale: 0.6,
        color: "#6b7280",
        y: 170.0,
        rise: 30.0,
        ..Text::default()
    });
    out.push(Template {
        id: "quote-card",
        name: "Quote card",
        description: "A square quote with its attribution, for posts and carousels.",
        category: "Social",
        project: quote,
        preview: json!({"background": "#f5f0e6", "title": "“Make the thing you wish existed.”", "color": "#1f2937", "font": "Playfair Display", "italic": true}),
    });

    let mut countdown = base("Countdown", 1080, 1920, "#000000");
    for (index, number) in ["3", "2", "1"].iter().enumerate() {
        add_text(&mut countdown, number, Text {
            content: number,
            font: "Bebas Neue",
            at: index as f64,
            duration: 1.0,
            size: 30.0,
            pop: 0.25,
            ..Text::default()
        });
    }
    add_text(&mut countdown, "Go", Text {
        content: "GO!",
        font: "Bebas Neue",
        at: 3.0,
        duration: 1.0,
        size: 24.0,
        color: "#22c55e",
        pop: 0.2,
        ..Text::default()
    });
    out.push(Template {
        id: "countdown",
        name: "Countdown",
        description: "Three, two, one, go: a vertical countdown to open a reveal or a challenge.",
        category: "Social",
        project: countdown,
        preview: json!({"background": "#000000", "title": "3 2 1", "color": "#ffffff", "font": "Bebas Neue"}),
    });

    let mut lower = base("Lower third", 1920, 1080, "#334155");
    add_text(&mut lower, "Name", Text {
        content: "Alex Rivera",
        size: 5.5,
        bold: true,
        align: "left",
        x: -560.0,
        y: 330.0,
        box_color: Some("#2563eb"),
        rise: 40.0,
        duration: 5.0,
        ..Text::default()
    });
    add_text(&mut lower, "Role", Text {
        content: "Product designer",
        at: 0.3,
        duration: 4.7,
        size: 5.0,
        scale: 0.7,
        align: "left",
        color: "#e2e8f0",
        x: -580.0,
        y: 430.0,
        rise: 30.0,
        ..Text::default()
    });
    out.push(Template {
        id: "lower-third",
        name: "Lower third",
        description: "A name and role for interviews. Put your clip on the main track under it.",
        category: "Titles",
        project: lower,
        preview: json!({"background": "#334155", "title": "Alex Rivera", "color": "#ffffff", "box": "#2563eb", "font": "Inter", "lowerThird": true}),
    });
    out
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/public/templates");
    std::fs::create_dir_all(&dir).expect("templates dir");
    let mut index = Vec::new();
    for mut template in templates() {
        // Stable ids, so a rebuild only changes what changed.
        template.project.id = format!("template-{}", template.id);
        for (track_index, track) in template.project.tracks.iter_mut().enumerate() {
            track.id = format!("{}-track-{track_index}", template.id);
            for (element_index, element) in track.elements.iter_mut().enumerate() {
                element.id = format!("{}-{track_index}-{element_index}", template.id);
            }
        }
        let file = format!("{}.opencut.zip", template.id);
        export_bundle(&template.project, &dir.join(&file)).expect("export template");
        let canvas = template.project.settings.canvas;
        let fonts: std::collections::BTreeSet<String> = template
            .project
            .tracks
            .iter()
            .flat_map(|track| track.elements.iter())
            .map(|element| element.params.text("fontFamily", "Arial").into_owned())
            .collect();
        index.push(json!({
            "id": template.id,
            "name": template.name,
            "description": template.description,
            "category": template.category,
            "file": file,
            "width": canvas.width,
            "height": canvas.height,
            "duration": template.project.duration().to_seconds_f64(),
            "fonts": fonts,
            "preview": template.preview,
        }));
        println!("{file}");
    }
    let index = json!({
        "format": "opencut-templates",
        "formatVersion": 1,
        "note": "Built by `cargo run -p editor --example build_templates`; ConzaCut's own designs.",
        "templates": index,
    });
    std::fs::write(dir.join("index.json"), serde_json::to_string_pretty(&index).expect("json") + "\n").expect("index");
}

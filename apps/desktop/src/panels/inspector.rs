use std::collections::HashMap;

use editor::{
    ParamValue,
    Background, CanvasSize, Command, Effect, ElementKind, ElementPatch, Mask, MaskSize, Params, SettingsPatch,
};
use gpui::{Context, Entity, FocusHandle, FontWeight, ScrollHandle, Window, div, prelude::*, px};
use time::{FrameRate, MediaTime};

use crate::components::{Button, ButtonSize, ButtonVariant, Separator, TextInput, eid};
use crate::format::timecode;
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// Properties of the selected element, or of the project when nothing is
/// selected. Every field is a text input that commits on Enter or blur.
/// Numeric parameters can be keyframed: the diamond next to a field adds
/// or removes a key at the playhead, and a keyed field edits that key.
pub(crate) struct InspectorPanel {
    state: Entity<EditorState>,
    editor_focus: FocusHandle,
    /// Text inputs by field key, created on first use.
    fields: HashMap<String, Entity<TextInput>>,
    scroll: ScrollHandle,
    /// Auto-ducking choices: the voice clip (none = every other clip) and
    /// how far the music drops.
    duck_voice: Option<String>,
    duck_amount_db: f64,
    /// Whisper model chosen for transcription.
    transcribe_model: &'static str,
    /// Silence auto-cut choices: the shortest pause that goes, and how the
    /// gaps close.
    silence_min_seconds: f64,
    loudness_target: usize,
    shot_threshold: f64,
    beats_every: usize,
    denoise_db: f64,
    filler_language: &'static str,
    silence_ripple: editor::RippleScope,
}

/// Shortest pauses offered for silence auto-cut, in seconds.
const SILENCE_MIN_SECONDS: &[f64] = &[0.3, 0.5, 1.0, 2.0];

/// Drops offered for auto-ducking, in dB.
const DUCK_AMOUNTS: &[f64] = &[-6.0, -12.0, -18.0, -24.0];

/// Numeric element parameters shown under "Transform".
const NUMBER_PARAMS: &[(&str, &str, f64)] = &[
    ("transform.positionX", "Position X", 0.0),
    ("transform.positionY", "Position Y", 0.0),
    ("transform.scaleX", "Scale X", 1.0),
    ("transform.scaleY", "Scale Y", 1.0),
    ("transform.rotate", "Rotation", 0.0),
    ("opacity", "Opacity", 1.0),
];

const TEXT_NUMBER_PARAMS: &[(&str, &str, f64)] = &[
    ("fontSize", "Font size", 15.0),
    ("lineHeight", "Line height", 1.2),
    ("letterSpacing", "Letter spacing", 0.0),
];

/// Caption colours, shown for text elements that carry timed words.
const CAPTION_COLOR_PARAMS: &[(&str, &str, &str)] = &[
    ("caption.activeColor", "Active colour", "#ffd60a"),
    ("caption.boxColor", "Box colour", "#e5484d"),
];

const TEXT_STRING_PARAMS: &[(&str, &str, &str)] = &[
    ("content", "Text", "Default text"),
    ("fontFamily", "Font", "Arial"),
    ("color", "Color", "#ffffff"),
];

/// Mask settings shown as percentages of the element's size.
const MASK_PERCENT_PARAMS: &[(&str, &str)] = &[
    ("centerX", "X (%)"),
    ("centerY", "Y (%)"),
    ("width", "Width (%)"),
    ("height", "Height (%)"),
    ("scale", "Scale (%)"),
];

/// Where a parameter stands with respect to keyframes.
#[derive(Clone)]
struct KeyState {
    path: String,
    animated: bool,
    at_playhead: bool,
}

type Commit = Box<dyn Fn(&str, &mut EditorState, &mut Context<EditorState>) + 'static>;

impl InspectorPanel {
    pub(crate) fn new(state: Entity<EditorState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let editor_focus = state.read(cx).focus_handle.clone();
        Self {
            state,
            editor_focus,
            fields: HashMap::new(),
            scroll: ScrollHandle::new(),
            duck_voice: None,
            duck_amount_db: -12.0,
            transcribe_model: "base",
            silence_min_seconds: 0.5,
            loudness_target: 0,
            shot_threshold: 20.0,
            beats_every: 1,
            denoise_db: 12.0,
            filler_language: "auto",
            silence_ripple: editor::RippleScope::All,
        }
    }

    /// The input for `key`, created with `commit` on first use, showing
    /// `text` (unless it is being typed in).
    fn field(
        &mut self,
        key: &str,
        placeholder: &'static str,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
        commit: Commit,
    ) -> Entity<TextInput> {
        let field = match self.fields.get(key) {
            Some(field) => field.clone(),
            None => {
                let state = self.state.clone();
                let editor_focus = self.editor_focus.clone();
                let field = cx.new(|cx| {
                    TextInput::new(window, cx, placeholder)
                        .focus_after(editor_focus)
                        .on_commit(move |text, _, cx| {
                            state.update(cx, |state, cx| commit(text, state, cx));
                        })
                });
                self.fields.insert(key.to_owned(), field.clone());
                field
            }
        };
        field.update(cx, |field, cx| field.set_text(text, window, cx));
        field
    }

    /// A labelled field, with keyframe buttons when `key_state` is given.
    fn row(
        &mut self,
        label: &'static str,
        key: &str,
        placeholder: &'static str,
        text: String,
        key_state: Option<KeyState>,
        window: &mut Window,
        cx: &mut Context<Self>,
        commit: Commit,
    ) -> gpui::Div {
        let colors = window.theme().colors;
        let field = self.field(key, placeholder, text, window, cx, commit);
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .w(px(88.))
                    .flex_none()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(div().flex_1().child(field));
        if let Some(key_state) = key_state {
            let path = key_state.path.clone();
            let state = self.state.clone();
            row = row.child(
                Button::new(eid("key", &path), "◆")
                    .variant(if key_state.at_playhead {
                        ButtonVariant::Default
                    } else if key_state.animated {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::IconXSmall)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| state.toggle_keyframe(&path, cx))
                    }),
            );
            if key_state.animated {
                let path = key_state.path.clone();
                let state = self.state.clone();
                row = row.child(
                    Button::new(eid("unkey", &path), "×")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXSmall)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |state, cx| state.clear_animation(&path, cx))
                        }),
                );
            }
        }
        row
    }

    /// The clip's transition out: pick one of the catalogue by name (a
    /// field that accepts a prefix, Previous/Next, a few common picks),
    /// set its length in seconds and its parameters, or remove it.
    fn transition_section<C: ParentElement>(
        &mut self,
        mut content: C,
        element: &editor::Element,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> C {
        let colors = window.theme().colors;
        let catalogue = compositor::transition_catalogue();
        let current = element.transition.clone();
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(self.heading("Transition", window));
        if current.is_some() {
            let state = self.state.clone();
            header = header.child(
                Button::new("transition-remove", "Remove")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::XSmall)
                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.remove_transition(cx))),
            );
        }
        content = content.child(Separator::horizontal()).child(header);
        let names: Vec<&'static str> = catalogue.iter().map(|info| info.name).collect();
        let kind_row = self.row(
            "Type",
            "transition.kind",
            "None (type a name)",
            current.as_ref().map(|transition| transition.kind.clone()).unwrap_or_default(),
            None,
            window,
            cx,
            Box::new({
                let names = names.clone();
                move |text, state, cx| {
                    let wanted = text.trim().to_ascii_lowercase();
                    if wanted.is_empty() {
                        state.remove_transition(cx);
                        return;
                    }
                    let found = names
                        .iter()
                        .find(|name| name.to_ascii_lowercase() == wanted)
                        .or_else(|| names.iter().find(|name| name.to_ascii_lowercase().starts_with(&wanted)))
                        .or_else(|| names.iter().find(|name| name.to_ascii_lowercase().contains(&wanted)));
                    match found {
                        Some(name) => state.set_transition_kind(name, cx),
                        None => state.set_status(format!("No transition called \"{}\"", text.trim()), cx),
                    }
                }
            }),
        );
        content = content.child(kind_row);
        let index = current
            .as_ref()
            .and_then(|transition| names.iter().position(|name| *name == transition.kind));
        let step = |offset: isize| -> &'static str {
            let count = names.len() as isize;
            let base = index.map(|index| index as isize).unwrap_or(-1);
            names[((base + offset).rem_euclid(count)) as usize]
        };
        let previous = step(-1);
        let next = step(1);
        let mut picks = div().flex().flex_wrap().items_center().gap(px(4.));
        for (id, label, name) in [
            ("transition-prev", "‹ Previous", previous),
            ("transition-next", "Next ›", next),
            ("transition-fade", "Fade", "Fade"),
            ("transition-wipe", "Wipe", "DirectionalWipe"),
            ("transition-zoom", "Zoom", "CrossZoom"),
            ("transition-cube", "Cube", "Cube"),
            ("transition-slide", "Slide", "Directional"),
        ] {
            let state = self.state.clone();
            let active = current.as_ref().is_some_and(|transition| transition.kind == name)
                && !id.ends_with("prev")
                && !id.ends_with("next");
            picks = picks.child(
                Button::new(id, label)
                    .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.set_transition_kind(name, cx))),
            );
        }
        content = content.child(picks);
        let Some(transition) = current else {
            return content.child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(format!(
                        "No transition. One plays centred on the cut to the next clip ({} to choose from).",
                        names.len()
                    )),
            );
        };
        let duration_row = self.row(
            "Duration",
            "transition.duration",
            "Seconds",
            format!("{:.2}", transition.duration.to_seconds_f64()),
            None,
            window,
            cx,
            Box::new(|text, state, cx| {
                if let Ok(seconds) = text.trim().parse::<f64>()
                    && seconds > 0.0
                    && let Some(duration) = MediaTime::from_seconds_f64(seconds)
                {
                    state.update_transition(Some(duration), None, cx);
                }
            }),
        );
        content = content.child(duration_row);
        if let Some(info) = compositor::transition_info(&transition.kind) {
            if let Some(description) = info.description {
                content = content.child(div().text_xs().text_color(colors.muted_foreground).child(description.to_owned()));
            }
            for param in info.params {
                let stored = transition.params.get(param.name).cloned();
                let shown = match (&stored, &param.default) {
                    (Some(ParamValue::Bool(value)), _) => value.to_string(),
                    (Some(ParamValue::Number(value)), _) => format!("{value}"),
                    (Some(ParamValue::Json(serde_json::Value::Array(items))), _) => items
                        .iter()
                        .filter_map(serde_json::Value::as_f64)
                        .map(|value| format!("{value}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    (_, compositor::TransitionParamDescriptor::Bool(value)) => value.to_string(),
                    (_, compositor::TransitionParamDescriptor::Number(value)) => format!("{value}"),
                    (_, compositor::TransitionParamDescriptor::Vector(values)) => values
                        .iter()
                        .map(|value| format!("{value}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                let kind = param.kind;
                let name = param.name;
                let placeholder: &'static str = match kind {
                    "bool" => "true or false",
                    "float" | "int" => "Number",
                    _ => "Numbers, comma separated",
                };
                let row = self.row(
                    name,
                    &format!("transition.param.{name}"),
                    placeholder,
                    shown,
                    None,
                    window,
                    cx,
                    Box::new(move |text, state, cx| {
                        let text = text.trim();
                        let mut params = Params::new();
                        let value: Option<ParamValue> = match kind {
                            "bool" => match text.to_ascii_lowercase().as_str() {
                                "true" | "1" | "yes" | "on" => Some(ParamValue::Bool(true)),
                                "false" | "0" | "no" | "off" => Some(ParamValue::Bool(false)),
                                _ => None,
                            },
                            "float" | "int" => text.parse::<f64>().ok().map(ParamValue::Number),
                            _ => {
                                let numbers: Option<Vec<f64>> =
                                    text.split(',').map(|part| part.trim().parse::<f64>().ok()).collect();
                                numbers.map(|numbers| ParamValue::Json(serde_json::Value::Array(
                                    numbers.into_iter().filter_map(serde_json::Number::from_f64).map(serde_json::Value::Number).collect(),
                                )))
                            }
                        };
                        match value {
                            Some(value) => {
                                params.set(name, value);
                                state.update_transition(None, Some(params), cx);
                            }
                            None => state.set_status(format!("{name} must be a {kind}"), cx),
                        }
                    }),
                );
                content = content.child(row);
            }
        }
        content
    }

    fn heading(&self, text: &'static str, window: &Window) -> gpui::Div {
        let colors = window.theme().colors;
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.foreground)
            .child(text)
    }

    fn label_cell(text: impl Into<gpui::SharedString>, window: &Window) -> gpui::Div {
        let colors = window.theme().colors;
        div()
            .w(px(88.))
            .flex_none()
            .text_xs()
            .text_color(colors.muted_foreground)
            .child(text.into())
    }
}

fn number_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{}", (value * 1000.0).round() / 1000.0)
    }
}

/// A commit handler that parses a number and sets one element parameter.
fn set_number_param(param: &'static str) -> Commit {
    Box::new(move |text, state, cx| {
        if let Ok(value) = text.trim().parse::<f64>() {
            let mut params = Params::new();
            params.set(param, value);
            state.update_selected_params(params, cx);
        }
    })
}

fn set_text_param(param: &'static str) -> Commit {
    Box::new(move |text, state, cx| {
        let mut params = Params::new();
        params.set(param, text.to_owned());
        state.update_selected_params(params, cx);
    })
}

impl Render for InspectorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;
        let state = self.state.read(cx);
        let project = state.project();
        let rate = project.settings.fps;
        let playhead = state.playhead();
        let selected = state.selected_element().cloned();
        let selection_count = state.selection.len();
        let pen_active = state.pen.is_some();
        let track_kind = selected
            .as_ref()
            .and_then(|element| project.element(&element.id))
            .map(|(track, _)| track.kind);
        let media_has_audio = selected
            .as_ref()
            .and_then(|element| project.media_for(element))
            .map(|media| media.has_audio)
            .unwrap_or(false);
        let project_name = project.name.clone();
        let (background, blur_intensity) = match &project.settings.background {
            Background::Color { color } => (color.clone(), None),
            Background::Blur { blur_intensity } => ("#000000".to_owned(), Some(*blur_intensity)),
        };
        let canvas = project.settings.canvas;
        let has_ffmpeg = state.has_ffmpeg();
        let loudness = state.loudness.clone();
        let beats = state.beats;
        let marker_count = state.markers.len();
        // Voice choices for auto-ducking: every other clip with audio.
        let duck_others: Vec<(String, String)> = selected
            .as_ref()
            .map(|element| {
                project
                    .tracks
                    .iter()
                    .flat_map(|track| track.elements.iter())
                    .filter(|other| other.id != element.id)
                    .filter(|other| match &other.kind {
                        ElementKind::Audio { .. } => true,
                        ElementKind::Video { .. } => project.media_for(other).is_some_and(|media| media.has_audio),
                        _ => false,
                    })
                    .map(|other| (other.id.clone(), other.name.clone()))
                    .collect()
            })
            .unwrap_or_default();

        // Drop inputs that belonged to another element's effects and masks.
        if let Some(element) = &selected {
            let effect_ids: Vec<String> = element.effects.iter().map(|effect| effect.id.clone()).collect();
            let mask_ids: Vec<String> = element.masks.iter().map(|mask| mask.id.clone()).collect();
            self.fields.retain(|key, _| {
                if let Some(rest) = key.strip_prefix("effect.") {
                    effect_ids.iter().any(|id| rest.starts_with(id.as_str()))
                } else if let Some(rest) = key.strip_prefix("mask.") {
                    mask_ids.iter().any(|id| rest.starts_with(id.as_str()))
                } else {
                    true
                }
            });
        }

        let mut content = div()
            .id("inspector-content")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(px(12.))
            .gap(px(10.));
        if crate::state::trace_input() {
            eprintln!(
                "trace: inspector scroll offset={:?} max={:?} bounds={:?}",
                self.scroll.offset(),
                self.scroll.max_offset(),
                self.scroll.bounds()
            );
        }

        match selected {
            Some(element) => {
                let kind = element.kind.label();
                let is_text = matches!(element.kind, ElementKind::Text);
                let has_audio = matches!(element.kind, ElementKind::Audio { .. })
                    || (matches!(element.kind, ElementKind::Video { .. }) && media_has_audio);
                let is_visual = element.kind.is_visual();
                let is_compound = matches!(element.kind, editor::ElementKind::Compound { .. });
                // A compound has no picture of its own to transform or grade.
                let has_picture = is_visual && !is_compound;
                let group_size = element
                    .group_id
                    .as_ref()
                    .map(|_| editor::group_members(state.project(), &element.id).len())
                    .unwrap_or(0);
                let compound_summary = match &element.kind {
                    editor::ElementKind::Compound { tracks } => Some((editor::compound_child_count(&element), tracks.len())),
                    _ => None,
                };
                let can_mask = element.can_be_masked();
                let muted = element.params.bool("muted", false);
                let hidden = element.hidden;
                let align = element.params.text("textAlign", "center").into_owned();
                let bold = element.params.text("fontWeight", "normal").as_ref() == "bold";
                let italic = element.params.text("fontStyle", "normal").as_ref() == "italic";
                // Values at the playhead, so keyframed fields follow the motion.
                let params = element.params_at(playhead);
                let local = element.local_time(playhead);
                let key_state = |path: &str| -> KeyState {
                    let channel = element.animations.get(path);
                    KeyState {
                        path: path.to_owned(),
                        animated: channel.is_some_and(|channel| !channel.is_empty()),
                        at_playhead: channel.is_some_and(|channel| channel.key_at(local).is_some()),
                    }
                };
                let key_count: usize = element
                    .animations
                    .values()
                    .map(|channel| channel.all_keys().count())
                    .sum();

                let name_row = self.row(
                    "Name",
                    "element.name",
                    "Name",
                    element.name.clone(),
                    None,
                    window,
                    cx,
                    Box::new(|text, state, cx| {
                        let name = text.trim().to_owned();
                        if !name.is_empty() {
                            state.patch_selected(
                                ElementPatch {
                                    name: Some(name),
                                    ..Default::default()
                                },
                                cx,
                            );
                        }
                    }),
                );
                content = content
                    .child(self.heading("Element", window))
                    .child(name_row)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child(format!(
                                "{kind} on a {} track, {} long",
                                track_kind.map(|k| k.label()).unwrap_or("?"),
                                timecode(element.duration, rate)
                            ))
                            .child(format!(
                                "{} → {}",
                                timecode(element.start_time, rate),
                                timecode(element.end_time(), rate)
                            ))
                            .when(key_count > 0, |this| {
                                this.child(format!(
                                    "{key_count} keyframe{} · Shift+←/→ steps through them",
                                    if key_count == 1 { "" } else { "s" }
                                ))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child({
                                let state = self.state.clone();
                                Button::new("split", "Split at playhead")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.split_at_playhead(cx))
                                    })
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("duplicate", "Duplicate")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.duplicate_selection(cx))
                                    })
                            })
                            .when(is_visual, |this| {
                                let state = self.state.clone();
                                this.child(
                                    Button::new("hide", if hidden { "Show" } else { "Hide" })
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::Small)
                                        .on_click(move |_, _, cx| {
                                            state.update(cx, |state, cx| {
                                                state.patch_selected(
                                                    ElementPatch {
                                                        hidden: Some(!hidden),
                                                        ..Default::default()
                                                    },
                                                    cx,
                                                )
                                            })
                                        }),
                                )
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("delete", "Delete")
                                    .variant(ButtonVariant::Destructive)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.delete_selection(cx))
                                    })
                            }),
                    );

                if let Some((clips, tracks)) = compound_summary {
                    let state = self.state.clone();
                    content = content
                        .child(self.heading("Compound clip", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(format!("{clips} clip(s) on {tracks} track(s). Trim, split and move it like one clip; break it apart to edit what is inside.")),
                        )
                        .child(
                            Button::new("break-apart", "Break apart")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| state.update(cx, |state, cx| state.break_apart_selected(cx))),
                        );
                }
                if group_size > 1 {
                    let state = self.state.clone();
                    content = content.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(colors.muted_foreground)
                                    .child(format!("In a group of {group_size}; Alt-click picks one clip")),
                            )
                            .child(
                                Button::new("ungroup-one", "Ungroup")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.ungroup_selection(cx))),
                            ),
                    );
                }
                if has_picture {
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Transform", window));
                    for (key, label, default) in NUMBER_PARAMS {
                        let row = self.row(
                            label,
                            key,
                            "",
                            number_text(params.number(key, *default)),
                            Some(key_state(key)),
                            window,
                            cx,
                            set_number_param(key),
                        );
                        content = content.child(row);
                    }
                }

                if is_text {
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Text", window));
                    for (key, label, default) in TEXT_STRING_PARAMS {
                        // The colour can be keyframed (as four channels, like the web app).
                        let keyable = *key == "color";
                        let row = self.row(
                            label,
                            key,
                            default,
                            params.text(key, default).into_owned(),
                            keyable.then(|| key_state(key)),
                            window,
                            cx,
                            set_text_param(key),
                        );
                        content = content.child(row);
                    }
                    // A family the machine lacks can be fetched from Fontsource
                    // (open licences, files kept beside the projects).
                    let family = params.text("fontFamily", "Arial").into_owned();
                    content = content.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Self::label_cell("Font files", window))
                            .child({
                                let state = self.state.clone();
                                let label = format!("Fetch {family} from Fontsource");
                                Button::new("fetch-font", label)
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::XSmall)
                                    .on_click(move |_, _, cx| {
                                        let family = family.clone();
                                        state.update(cx, |state, cx| state.fetch_font(family, cx));
                                    })
                            }),
                    );
                    for (key, label, default) in TEXT_NUMBER_PARAMS {
                        let row = self.row(
                            label,
                            key,
                            "",
                            number_text(params.number(key, *default)),
                            Some(key_state(key)),
                            window,
                            cx,
                            set_number_param(key),
                        );
                        content = content.child(row);
                    }
                    let mut align_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(Self::label_cell("Align", window));
                    for option in ["left", "center", "right"] {
                        let state = self.state.clone();
                        let active = align == option;
                        align_row = align_row.child(
                            Button::new(eid("align", option), option)
                                .variant(if active {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Outline
                                })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let mut params = Params::new();
                                    params.set("textAlign", option);
                                    state.update(cx, |state, cx| state.update_selected_params(params, cx));
                                }),
                        );
                    }
                    let style_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(Self::label_cell("Style", window))
                        .child({
                            let state = self.state.clone();
                            Button::new("bold", "Bold")
                                .variant(if bold {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Outline
                                })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let mut params = Params::new();
                                    params.set("fontWeight", if bold { "normal" } else { "bold" });
                                    state.update(cx, |state, cx| state.update_selected_params(params, cx));
                                })
                        })
                        .child({
                            let state = self.state.clone();
                            Button::new("italic", "Italic")
                                .variant(if italic {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Outline
                                })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let mut params = Params::new();
                                    params.set("fontStyle", if italic { "normal" } else { "italic" });
                                    state.update(cx, |state, cx| state.update_selected_params(params, cx));
                                })
                        });
                    content = content.child(align_row).child(style_row);
                }

                if is_text && !element.words.is_empty() {
                    let word_count = element.words.len();
                    let highlight = params.text("caption.highlight", "none").into_owned();
                    let mut highlight_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(Self::label_cell("Highlight", window));
                    for (option, label) in [("none", "None"), ("word", "Word"), ("box", "Box"), ("fill", "Fill")] {
                        let state = self.state.clone();
                        let active = highlight == option;
                        highlight_row = highlight_row.child(
                            Button::new(eid("caption-highlight", option), label)
                                .variant(if active {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Outline
                                })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let mut params = Params::new();
                                    params.set("caption.highlight", option);
                                    state.update(cx, |state, cx| state.update_selected_params(params, cx));
                                }),
                        );
                    }
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Captions", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(format!("{word_count} timed word(s); the spoken one is emphasised.")),
                        )
                        .child(highlight_row);
                    for (key, label, default) in CAPTION_COLOR_PARAMS {
                        let row = self.row(
                            label,
                            key,
                            default,
                            params.text(key, default).into_owned(),
                            None,
                            window,
                            cx,
                            set_text_param(key),
                        );
                        content = content.child(row);
                    }
                }

                if has_audio {
                    let volume_row = self.row(
                        "Volume (dB)",
                        "volume",
                        "0",
                        number_text(params.number("volume", 0.0)),
                        Some(key_state("volume")),
                        window,
                        cx,
                        Box::new(|text, state, cx| {
                            if let Ok(value) = text.trim().parse::<f64>() {
                                let mut params = Params::new();
                                params.set("volume", value.clamp(-60.0, 20.0));
                                state.update_selected_params(params, cx);
                            }
                        }),
                    );
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Audio", window))
                        .child(volume_row)
                        .child({
                            let state = self.state.clone();
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(Self::label_cell("Mute", window))
                                .child(
                                    Button::new("mute", if muted { "Muted" } else { "On" })
                                        .variant(if muted {
                                            ButtonVariant::Default
                                        } else {
                                            ButtonVariant::Outline
                                        })
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            let mut params = Params::new();
                                            params.set("muted", !muted);
                                            state.update(cx, |state, cx| state.update_selected_params(params, cx));
                                        }),
                                )
                        });

                    // Auto-ducking: the clip's volume dips while the voice
                    // (every other audio clip, or one chosen here) speaks.
                    let others = duck_others.clone();
                    if !others.iter().any(|(id, _)| Some(id) == self.duck_voice.as_ref()) {
                        self.duck_voice = None;
                    }
                    let voice_label = self
                        .duck_voice
                        .as_ref()
                        .and_then(|id| others.iter().find(|(other, _)| other == id))
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| "All other clips".to_owned());
                    let mut voice_row = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(Self::label_cell("Voice", window));
                    if others.is_empty() {
                        voice_row = voice_row.child(
                            div().text_xs().text_color(colors.muted_foreground).child("Add a voice clip to duck under."),
                        );
                    } else {
                        // Each click moves to the next voice choice, wrapping
                        // round to "all other clips".
                        let next_voice: Option<String> = match &self.duck_voice {
                            None => others.first().map(|(id, _)| id.clone()),
                            Some(current) => {
                                let index = others.iter().position(|(id, _)| id == current).unwrap_or(0);
                                others.get(index + 1).map(|(id, _)| id.clone())
                            }
                        };
                        voice_row = voice_row.child(
                            Button::new("duck-voice", voice_label)
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.duck_voice = next_voice.clone();
                                    cx.notify();
                                })),
                        );
                    }
                    let mut amount_row = div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Drop (dB)", window));
                    for amount in DUCK_AMOUNTS {
                        let amount = *amount;
                        let active = (self.duck_amount_db - amount).abs() < 0.01;
                        let label: &'static str = match amount as i32 {
                            -6 => "-6",
                            -12 => "-12",
                            -18 => "-18",
                            _ => "-24",
                        };
                        amount_row = amount_row.child(
                            Button::new(eid("duck-amount", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.duck_amount_db = amount;
                                    cx.notify();
                                })),
                        );
                    }
                    let ducked = element.animations.get("volume").is_some_and(|channel| !channel.is_empty());
                    let can_duck = !others.is_empty() && has_ffmpeg;
                    let voice_id = self.duck_voice.clone();
                    let amount_db = self.duck_amount_db;
                    let action_row = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child({
                            let state = self.state.clone();
                            Button::new("duck", "Duck under voice")
                                .size(ButtonSize::XSmall)
                                .disabled(!can_duck)
                                .on_click(move |_, _, cx| {
                                    let voice_id = voice_id.clone();
                                    state.update(cx, |state, cx| state.duck_selected(voice_id, amount_db, cx));
                                })
                        })
                        .child({
                            let state = self.state.clone();
                            Button::new("unduck", "Clear")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .disabled(!ducked)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.clear_animation("volume", cx));
                                })
                        });
                    content = content
                        .child(self.heading("Ducking", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Dips this clip while speech plays: down 0.1 s before, up 0.5 s after."),
                        )
                        .child(voice_row)
                        .child(amount_row)
                        .child(action_row);

                    // Silence auto-cut: pauses under -40 dBFS longer than the
                    // chosen length go, with 0.1 s kept next to the speech.
                    let mut min_row = div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Longer than", window));
                    for seconds in SILENCE_MIN_SECONDS {
                        let seconds = *seconds;
                        let active = (self.silence_min_seconds - seconds).abs() < 0.01;
                        let label: &'static str = match (seconds * 10.0) as i32 {
                            3 => "0.3 s",
                            5 => "0.5 s",
                            10 => "1 s",
                            _ => "2 s",
                        };
                        min_row = min_row.child(
                            Button::new(eid("silence-min", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.silence_min_seconds = seconds;
                                    cx.notify();
                                })),
                        );
                    }
                    let ripple = self.silence_ripple;
                    let (ripple_label, next_ripple) = match ripple {
                        editor::RippleScope::All => ("Close gaps on every track", editor::RippleScope::Track),
                        editor::RippleScope::Track => ("Close gaps on this track", editor::RippleScope::None),
                        editor::RippleScope::None => ("Leave the gaps", editor::RippleScope::All),
                    };
                    let ripple_row = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(Self::label_cell("Afterwards", window))
                        .child(
                            Button::new("silence-ripple", ripple_label)
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.silence_ripple = next_ripple;
                                    cx.notify();
                                })),
                        );
                    let min_seconds = self.silence_min_seconds;
                    let silence_row = div().flex().items_center().gap(px(8.)).child({
                        let state = self.state.clone();
                        Button::new("cut-silence", "Cut silence")
                            .size(ButtonSize::XSmall)
                            .disabled(!has_ffmpeg)
                            .on_click(move |_, _, cx| {
                                let options = editor::SilenceOptions { min_silence_seconds: min_seconds, ..Default::default() };
                                state.update(cx, |state, cx| state.cut_silence_selected(options, ripple, cx));
                            })
                    });
                    // Native speech-to-text through whisper.cpp.
                    let mut model_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Model", window));
                    for model in crate::transcribe::MODELS {
                        let active = self.transcribe_model == model.id;
                        let id = model.id;
                        model_row = model_row.child(
                            Button::new(eid("whisper-model", id), model.label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.transcribe_model = id;
                                    cx.notify();
                                })),
                        );
                    }
                    let model_id = self.transcribe_model;
                    let transcribe_row = div().flex().items_center().gap(px(8.)).child({
                        let state = self.state.clone();
                        Button::new("transcribe", "Transcribe to captions")
                            .size(ButtonSize::XSmall)
                            .disabled(!has_ffmpeg)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| state.transcribe_selected(model_id, cx));
                            })
                    });
                    // Filler words: cut "um", "uh" and friends out of the clip
                    // from the captions' word timings, or a prompted transcription.
                    let mut language_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Fillers", window));
                    for (code, label) in editor::FILLER_LANGUAGES {
                        let active = self.filler_language == *code;
                        language_row = language_row.child(
                            Button::new(eid("filler-language", code), *label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.filler_language = code;
                                    cx.notify();
                                })),
                        );
                    }
                    let filler_language = self.filler_language;
                    let filler_ripple = self.silence_ripple;
                    let fillers_row = div().flex().items_center().gap(px(8.)).child({
                        let state = self.state.clone();
                        Button::new("remove-fillers", "Remove filler words")
                            .size(ButtonSize::XSmall)
                            .disabled(!has_ffmpeg)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    state.remove_fillers_selected(model_id, filler_language.to_owned(), filler_ripple, cx)
                                });
                            })
                    });
                    content = content
                        .child(self.heading("Captions", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Word-timed captions from this clip's speech, on a new text track; the model is fetched once into ~/.opencut/models."),
                        )
                        .child(model_row)
                        .child(transcribe_row)
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Filler words are cut with 40 ms of padding (gaps close as the Silence setting says), from captions over the clip or a transcription."),
                        )
                        .child(language_row)
                        .child(fillers_row);

                    content = content
                        .child(self.heading("Silence", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Removes pauses under -40 dBFS, keeping 0.1 s next to the speech, in one undo step."),
                        )
                        .child(min_row)
                        .child(ripple_row)
                        .child(silence_row);

                    // Noise reduction: a denoised copy of the clip's audio
                    // (FFmpeg afftdn) becomes a new media item the clip plays.
                    let mut reduction_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Reduction", window));
                    for (label, value) in [("Light 6 dB", 6.0), ("Normal 12 dB", 12.0), ("Strong 20 dB", 20.0), ("Max 30 dB", 30.0)] {
                        let active = (self.denoise_db - value).abs() < 0.01;
                        reduction_row = reduction_row.child(
                            Button::new(eid("denoise-db", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.denoise_db = value;
                                    cx.notify();
                                })),
                        );
                    }
                    let reduction_db = self.denoise_db;
                    let state = self.state.clone();
                    content = content
                        .child(self.heading("Noise", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Subtracts the clip's steady background (hiss, hum, room tone) and writes a denoised copy the clip then plays; undo puts the original back."),
                        )
                        .child(reduction_row)
                        .child(
                            Button::new("reduce-noise", "Reduce noise")
                                .size(ButtonSize::XSmall)
                                .disabled(!has_ffmpeg)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.reduce_noise_selected(reduction_db, cx));
                                }),
                        );

                    // Beat markers: tempo from the clip's onsets; markers the
                    // timeline snaps to while clips are dragged.
                    let mut every_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Mark", window));
                    for (label, value) in [("Every beat", 1usize), ("Every 2nd", 2), ("Bars (4)", 4)] {
                        let active = self.beats_every == value;
                        every_row = every_row.child(
                            Button::new(eid("beats-every", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.beats_every = value;
                                    cx.notify();
                                })),
                        );
                    }
                    let every = self.beats_every;
                    let beats_actions = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child({
                            let state = self.state.clone();
                            Button::new("beats-find", "Find beats")
                                .size(ButtonSize::XSmall)
                                .disabled(!has_ffmpeg)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.detect_beats_selected(every, cx));
                                })
                        })
                        .child({
                            let state = self.state.clone();
                            Button::new("beats-clear", "Clear markers")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .disabled(marker_count == 0)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.clear_markers(cx));
                                })
                        });
                    content = content
                        .child(self.heading("Beats", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(match beats {
                                    Some((bpm, confidence)) => format!(
                                        "{bpm:.1} BPM, confidence {:.0}%, {marker_count} marker(s) on the ruler; clips snap to them.",
                                        confidence * 100.0
                                    ),
                                    None => "Finds the tempo (60–200 BPM) and puts markers on the beats; clips snap to markers while dragged.".to_owned(),
                                }),
                        )
                        .child(every_row)
                        .child(beats_actions);

                    // Loudness (EBU R128): measure the clip's source, or set
                    // its volume so the integrated loudness meets a target.
                    let report = loudness.as_ref().filter(|(id, _)| *id == element.id).map(|(_, report)| *report);
                    let mut target_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Target", window));
                    for (index, target) in editor::LOUDNESS_TARGETS.iter().enumerate() {
                        let active = self.loudness_target == index;
                        target_row = target_row.child(
                            Button::new(eid("loudness-target", target.id), target.label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.loudness_target = index;
                                    cx.notify();
                                })),
                        );
                    }
                    let target_index = self.loudness_target;
                    let loudness_actions = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child({
                            let state = self.state.clone();
                            Button::new("loudness-measure", "Measure")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .disabled(!has_ffmpeg)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.measure_loudness_selected(cx));
                                })
                        })
                        .child({
                            let state = self.state.clone();
                            Button::new("loudness-normalize", "Normalise")
                                .size(ButtonSize::XSmall)
                                .disabled(!has_ffmpeg)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.normalize_loudness_selected(target_index, cx));
                                })
                        });
                    content = content
                        .child(self.heading("Loudness", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("EBU R128 on the clip's source after trims, before its volume. Normalise sets the volume to meet the target under a −1 dBTP ceiling."),
                        );
                    if let Some(report) = report {
                        let fmt = |value: f64, unit: &str| {
                            if value.is_finite() { format!("{value:.1} {unit}") } else { "silent".to_owned() }
                        };
                        content = content.child(div().text_xs().child(format!(
                            "Integrated {} · Range {:.1} LU · True peak {} · Max momentary {} · Max short-term {}",
                            fmt(report.integrated_lufs, "LUFS"),
                            report.loudness_range_lu,
                            fmt(report.true_peak_dbtp, "dBTP"),
                            fmt(report.max_momentary_lufs, "LUFS"),
                            fmt(report.max_short_term_lufs, "LUFS")
                        )));
                    }
                    content = content.child(target_row).child(loudness_actions);
                }

                if matches!(element.kind, ElementKind::Video { .. } | ElementKind::Audio { .. }) {
                    // Speed: a constant rate, a ramp preset or reverse; the
                    // clip's length follows so the same source still plays.
                    let current = element.retime.clone().unwrap_or_default();
                    let is_ramp = !current.ramp.is_empty();
                    let mut rate_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Speed", window));
                    for (label, rate) in [("0.25x", 0.25), ("0.5x", 0.5), ("1x", 1.0), ("2x", 2.0), ("4x", 4.0)] {
                        let active = !is_ramp && (current.rate - rate).abs() < 1e-6;
                        let state = self.state.clone();
                        let base = current.clone();
                        rate_row = rate_row.child(
                            Button::new(eid("speed-rate", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let next = editor::Retime { rate, ramp: Vec::new(), ..base.clone() };
                                    let next = if next.is_identity() { None } else { Some(next) };
                                    state.update(cx, |state, cx| state.set_retime_selected(next, cx));
                                }),
                        );
                    }
                    let mut ramp_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Ramp", window));
                    let duration = element.duration.to_seconds_f64().max(0.1);
                    let presets: [(&str, Vec<(f64, f64)>); 5] = [
                        ("Off", Vec::new()),
                        ("Ease in", vec![(0.0, 0.25), (duration, 1.0)]),
                        ("Ease out", vec![(0.0, 1.0), (duration, 0.25)]),
                        ("Slow middle", vec![(0.0, 1.0), (duration / 2.0, 0.25), (duration, 1.0)]),
                        ("Fast middle", vec![(0.0, 1.0), (duration / 2.0, 4.0), (duration, 1.0)]),
                    ];
                    for (label, keys) in presets {
                        let active = if keys.is_empty() {
                            !is_ramp
                        } else {
                            is_ramp
                                && current.ramp.len() == keys.len()
                                && current.ramp.iter().zip(keys.iter()).all(|(have, want)| (have.rate - want.1).abs() < 1e-6)
                        };
                        let state = self.state.clone();
                        let base = current.clone();
                        ramp_row = ramp_row.child(
                            Button::new(eid("speed-ramp", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let next = editor::Retime {
                                        ramp: keys.iter().map(|(time, rate)| editor::RampKey { time: *time, rate: *rate }).collect(),
                                        ..base.clone()
                                    };
                                    let next = if next.is_identity() { None } else { Some(next) };
                                    state.update(cx, |state, cx| state.set_retime_selected(next, cx));
                                }),
                        );
                    }
                    let reverse_row = div().flex().items_center().gap(px(8.)).child({
                        let state = self.state.clone();
                        let base = current.clone();
                        let reversed = current.reverse;
                        Button::new("speed-reverse", if reversed { "Reversed" } else { "Reverse" })
                            .variant(if reversed { ButtonVariant::Default } else { ButtonVariant::Outline })
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| {
                                let next = editor::Retime { reverse: !reversed, ..base.clone() };
                                let next = if next.is_identity() { None } else { Some(next) };
                                state.update(cx, |state, cx| state.set_retime_selected(next, cx));
                            })
                    });
                    content = content
                        .child(self.heading("Speed", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("A rate, a ramp between rates or backwards; audio follows without pitch correction. The clip's length changes to play the same source."),
                        )
                        .child(rate_row)
                        .child(ramp_row)
                        .child(reverse_row);
                }

                if has_picture {
                    content = self.transition_section(content, &element, window, cx);
                }

                if matches!(element.kind, ElementKind::Video { .. }) {
                    // Shot detection: FFmpeg's scdet scores every frame's
                    // difference from the last; cuts split the clip.
                    let mut threshold_row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .child(Self::label_cell("Sensitivity", window));
                    for (label, value) in [("High", 10.0), ("Normal", 20.0), ("Low", 35.0)] {
                        let active = (self.shot_threshold - value).abs() < 0.01;
                        threshold_row = threshold_row.child(
                            Button::new(eid("shot-threshold", label), label)
                                .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                .size(ButtonSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.shot_threshold = value;
                                    cx.notify();
                                })),
                        );
                    }
                    let threshold = self.shot_threshold;
                    let state = self.state.clone();
                    content = content
                        .child(self.heading("Shots", window))
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Finds scene changes in this clip and splits it there, in one undo step."),
                        )
                        .child(threshold_row)
                        .child(
                            Button::new("split-at-shots", "Split at shot changes")
                                .size(ButtonSize::XSmall)
                                .disabled(!has_ffmpeg)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.split_at_shots_selected(threshold, cx));
                                }),
                        );
                }

                if has_picture {
                    content = content
                        .child(Separator::horizontal())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(self.heading("Effects", window)),
                        );
                    let mut add_row = div().flex().flex_wrap().items_center().gap(px(4.)).child(
                        div().text_xs().text_color(colors.muted_foreground).mr(px(4.)).child("Add"),
                    );
                    for spec in editor::EFFECT_CATALOGUE {
                        let state = self.state.clone();
                        let kind = spec.kind;
                        add_row = add_row.child(
                            Button::new(eid("add-effect", kind), spec.label)
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    if let Some(effect) = Effect::from_catalogue(kind) {
                                        state.update(cx, |state, cx| state.add_effect(effect, cx));
                                    }
                                }),
                        );
                    }
                    content = content.child(add_row);
                    if element.effects.is_empty() {
                        content = content.child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("No effects. Number settings can be keyframed; grain and glitch move on their own."),
                        );
                    }
                    for effect in &element.effects {
                        let effect_id = effect.id.clone();
                        let enabled = effect.enabled;
                        let effect_params = element.effect_params_at(effect, playhead);
                        let header = div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(Self::label_cell(effect.label().to_owned(), window))
                            .child({
                                let state = self.state.clone();
                                let effect_id = effect_id.clone();
                                Button::new(eid("effect-on", &effect_id), if enabled { "On" } else { "Off" })
                                    .variant(if enabled {
                                        ButtonVariant::Default
                                    } else {
                                        ButtonVariant::Outline
                                    })
                                    .size(ButtonSize::XSmall)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| {
                                            state.update_effect(&effect_id, None, Some(!enabled), cx)
                                        })
                                    })
                            })
                            .child({
                                let state = self.state.clone();
                                let effect_id = effect_id.clone();
                                Button::new(eid("effect-remove", &effect_id), "Remove")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::XSmall)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.remove_effect(&effect_id, cx))
                                    })
                            });
                        content = content.child(header);
                        if effect.kind == "lut" {
                            // The table itself: loaded from a file, not typed.
                            let param = effect.params.text("lut", "").into_owned();
                            let label = match editor::decode_lut_param(&param) {
                                Ok((name, table)) => format!("{name} ({}³)", table.size),
                                Err(_) if param.is_empty() => "No table loaded".to_owned(),
                                Err(_) => "Unreadable table".to_owned(),
                            };
                            let mut lut_row = div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(Self::label_cell("Table", window))
                                .child(div().text_xs().text_color(colors.muted_foreground).child(label))
                                .child({
                                    let state = self.state.clone();
                                    let effect_id = effect_id.clone();
                                    Button::new(eid("lut-load", &effect_id), "Load…")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            state.update(cx, |state, cx| state.load_lut_for_effect(&effect_id, cx))
                                        })
                                });
                            if !param.is_empty() {
                                let state = self.state.clone();
                                let effect_id = effect_id.clone();
                                lut_row = lut_row.child(
                                    Button::new(eid("lut-clear", &effect_id), "Clear")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            let mut params = Params::new();
                                            params.set("lut", "");
                                            state.update(cx, |state, cx| state.update_effect(&effect_id, Some(params), None, cx))
                                        }),
                                );
                            }
                            content = content.child(lut_row);
                        }
                        let Some(spec) = editor::effect_spec(&effect.kind) else { continue };
                        for param in spec.params {
                            if param.is_choice() {
                                // A choice: one button per option.
                                let current = effect_params.text(param.key, param.options[0].0).into_owned();
                                let mut choice_row = div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(Self::label_cell(param.label, window));
                                for (value, label) in param.options {
                                    let state = self.state.clone();
                                    let effect_id = effect_id.clone();
                                    let active = current == *value;
                                    let key = param.key;
                                    choice_row = choice_row.child(
                                        Button::new(eid(&format!("effect-{effect_id}-{key}"), value), *label)
                                            .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                                            .size(ButtonSize::XSmall)
                                            .on_click(move |_, _, cx| {
                                                let mut params = Params::new();
                                                params.set(key, *value);
                                                state.update(cx, |state, cx| {
                                                    state.update_effect(&effect_id, Some(params), None, cx)
                                                });
                                            }),
                                    );
                                }
                                content = content.child(choice_row);
                                continue;
                            }
                            let path = format!("effects.{effect_id}.params.{}", param.key);
                            let effect_id = effect_id.clone();
                            let (key, min, max) = (param.key, param.min, param.max);
                            let placeholder: &'static str = "";
                            let row = self.row(
                                param.label,
                                &format!("effect.{effect_id}.{key}"),
                                placeholder,
                                number_text(effect_params.number(key, param.default)),
                                Some(key_state(&path)),
                                window,
                                cx,
                                Box::new(move |text, state, cx| {
                                    if let Ok(value) = text.trim().parse::<f64>() {
                                        let mut params = Params::new();
                                        params.set(key, value.clamp(min, max));
                                        state.update_effect(&effect_id, Some(params), None, cx);
                                    }
                                }),
                            );
                            content = content.child(row);
                        }
                    }
                }

                if can_mask {
                    let mask_header = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(self.heading("Mask", window));
                    content = content.child(Separator::horizontal()).child(mask_header);
                    if element.masks.is_empty() {
                        content = content.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.))
                                .children(Mask::SHAPES.iter().map(|kind| {
                                    let state = self.state.clone();
                                    let sample = Mask::shape(kind);
                                    Button::new(eid("add-mask", kind), sample.label().to_owned())
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            state.update(cx, |state, cx| state.add_mask(Mask::shape(kind), cx))
                                        })
                                }))
                                .child({
                                    let state = self.state.clone();
                                    Button::new("draw-mask-path", "Draw path (pen)")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.start_pen(cx)))
                                }),
                        );
                    }
                    if let Some(mask) = element.masks.first() {
                        let mask_id = mask.id.clone();
                        let inverted = mask.params.bool("inverted", false);
                        let header = div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(Self::label_cell(mask.label().to_owned(), window))
                            .child({
                                let state = self.state.clone();
                                let mask_id = mask_id.clone();
                                Button::new(eid("mask-invert", &mask_id), if inverted { "Inverted" } else { "Invert" })
                                    .variant(if inverted {
                                        ButtonVariant::Default
                                    } else {
                                        ButtonVariant::Outline
                                    })
                                    .size(ButtonSize::XSmall)
                                    .on_click(move |_, _, cx| {
                                        let mut params = Params::new();
                                        params.set("inverted", !inverted);
                                        state.update(cx, |state, cx| state.update_mask(&mask_id, params, cx))
                                    })
                            })
                            .child({
                                let state = self.state.clone();
                                let mask_id = mask_id.clone();
                                Button::new(eid("mask-remove", &mask_id), "Remove")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::XSmall)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.remove_mask(&mask_id, cx))
                                    })
                            });
                        content = content.child(header);
                        if mask.kind == Mask::FREEFORM {
                            let points = editor::read_freeform_path(&mask.params).len();
                            let closed = mask.params.bool("closed", false);
                            let mut pen_row = div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(6.))
                                .child(div().text_xs().text_color(colors.muted_foreground).child(format!(
                                    "{points} point(s), {}",
                                    if pen_active { "drawing" } else if closed { "closed" } else { "open" }
                                )));
                            if pen_active {
                                let state = self.state.clone();
                                pen_row = pen_row.child(
                                    Button::new("pen-close", "Close path")
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.finish_pen(cx))),
                                );
                                let state = self.state.clone();
                                pen_row = pen_row.child(
                                    Button::new("pen-cancel", "Cancel")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.cancel_pen(cx))),
                                );
                            } else {
                                let state = self.state.clone();
                                pen_row = pen_row.child(
                                    Button::new("pen-redraw", "Redraw")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.start_pen(cx))),
                                );
                            }
                            content = content.child(pen_row);
                        }
                        let size_mode = mask.size_mode();
                        let is_text = mask.kind == Mask::TEXT;
                        let shown = |key: &str| match key {
                            "width" => size_mode == MaskSize::WidthHeight,
                            "height" => size_mode != MaskSize::None,
                            "scale" => matches!(mask.kind.as_str(), Mask::TEXT | Mask::FREEFORM),
                            _ => true,
                        };
                        if is_text {
                            let content_row = self.row(
                                "Text",
                                &format!("mask.{}.content", mask.id),
                                "Mask",
                                mask.params.text("content", "").into_owned(),
                                None,
                                window,
                                cx,
                                Box::new({
                                    let mask_id = mask_id.clone();
                                    move |text, state, cx| {
                                        let mut params = Params::new();
                                        params.set("content", text.to_owned());
                                        state.update_mask(&mask_id, params, cx);
                                    }
                                }),
                            );
                            let size_row = self.row(
                                "Font size",
                                &format!("mask.{}.fontSize", mask.id),
                                "15",
                                number_text(mask.params.number("fontSize", 15.0)),
                                None,
                                window,
                                cx,
                                Box::new({
                                    let mask_id = mask_id.clone();
                                    move |text, state, cx| {
                                        if let Ok(value) = text.trim().parse::<f64>() {
                                            let mut params = Params::new();
                                            params.set("fontSize", value.max(0.1));
                                            state.update_mask(&mask_id, params, cx);
                                        }
                                    }
                                }),
                            );
                            content = content.child(content_row).child(size_row);
                        }
                        for (key, label) in MASK_PERCENT_PARAMS.iter().filter(|(key, _)| shown(key)) {
                            let mask_id = mask_id.clone();
                            let param = *key;
                            let row = self.row(
                                label,
                                &format!("mask.{}.{key}", mask.id),
                                "",
                                number_text(mask.params.number(key, 0.0) * 100.0),
                                None,
                                window,
                                cx,
                                Box::new(move |text, state, cx| {
                                    if let Ok(value) = text.trim().parse::<f64>() {
                                        let mut params = Params::new();
                                        params.set(param, value / 100.0);
                                        state.update_mask(&mask_id, params, cx);
                                    }
                                }),
                            );
                            content = content.child(row);
                        }
                        for (key, label) in [
                            ("rotation", "Rotation"),
                            ("feather", "Feather (px)"),
                            ("strokeWidth", "Stroke (px)"),
                        ] {
                            let mask_id = mask_id.clone();
                            let row = self.row(
                                label,
                                &format!("mask.{}.{key}", mask.id),
                                "0",
                                number_text(mask.params.number(key, 0.0)),
                                None,
                                window,
                                cx,
                                Box::new(move |text, state, cx| {
                                    if let Ok(value) = text.trim().parse::<f64>() {
                                        let mut params = Params::new();
                                        params.set(key, if key == "rotation" { value } else { value.max(0.0) });
                                        state.update_mask(&mask_id, params, cx);
                                    }
                                }),
                            );
                            content = content.child(row);
                        }
                        if mask.params.number("strokeWidth", 0.0) > 0.0 {
                            let color_row = self.row(
                                "Stroke color",
                                &format!("mask.{}.strokeColor", mask.id),
                                "#ffffff",
                                element.mask_params_at(mask, playhead).text("strokeColor", "#ffffff").into_owned(),
                                Some(key_state(&format!("masks.{}.params.strokeColor", mask.id))),
                                window,
                                cx,
                                Box::new({
                                    let mask_id = mask_id.clone();
                                    move |text, state, cx| {
                                        if editor::parse_hex_color(text).is_some() {
                                            let mut params = Params::new();
                                            params.set("strokeColor", text.trim().to_owned());
                                            state.update_mask(&mask_id, params, cx);
                                        }
                                    }
                                }),
                            );
                            let align = mask.params.text("strokeAlign", "center").into_owned();
                            let mut align_row = div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(6.))
                                .child(Self::label_cell("Stroke align", window));
                            for (label, value) in [("Inside", "inside"), ("Center", "center"), ("Outside", "outside")] {
                                let state = self.state.clone();
                                let mask_id = mask_id.clone();
                                align_row = align_row.child(
                                    Button::new(eid("stroke-align", &format!("{mask_id}-{value}")), label)
                                        .variant(if align == value {
                                            ButtonVariant::Default
                                        } else {
                                            ButtonVariant::Outline
                                        })
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            let mut params = Params::new();
                                            params.set("strokeAlign", value);
                                            state.update(cx, |state, cx| state.update_mask(&mask_id, params, cx))
                                        }),
                                );
                            }
                            content = content.child(color_row).child(align_row);
                        }
                    } else {
                        content = content.child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("No mask. A mask shows only part of the clip; sizes are percentages of the clip."),
                        );
                    }
                }
            }
            None if selection_count > 1 => {
                content = content
                    .child(self.heading("Selection", window))
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child(format!("{selection_count} elements selected")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child({
                                let state = self.state.clone();
                                Button::new("duplicate-many", "Duplicate")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.duplicate_selection(cx))
                                    })
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("group-many", "Group")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.group_selection(cx)))
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("ungroup-many", "Ungroup")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.ungroup_selection(cx)))
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("compound-many", "Compound")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| state.update(cx, |state, cx| state.create_compound(cx)))
                            })
                            .child({
                                let state = self.state.clone();
                                Button::new("delete-many", "Delete")
                                    .variant(ButtonVariant::Destructive)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |state, cx| state.delete_selection(cx))
                                    })
                            }),
                    );
            }
            None => {
                let name_row = self.row(
                    "Name",
                    "project.name",
                    "Project name",
                    project_name,
                    None,
                    window,
                    cx,
                    Box::new(|text, state, cx| state.rename_project(text.to_owned(), cx)),
                );
                let background_row = self.row(
                    "Background",
                    "project.background",
                    "#000000",
                    background.clone(),
                    None,
                    window,
                    cx,
                    Box::new(|text, state, cx| {
                        if editor::parse_hex_color(text).is_some() {
                            let color = text.trim().to_owned();
                            state.execute(
                                Command::UpdateSettings(SettingsPatch {
                                    background: Some(Background::Color { color }),
                                    ..Default::default()
                                }),
                                cx,
                            );
                        }
                    }),
                );
                // Colour, or the main track blurred behind the canvas (the
                // web app's presets).
                let mut blur_row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .child(Self::label_cell("Backdrop", window));
                for (label, preset) in [
                    ("Color", None),
                    ("Light blur", Some(100.0)),
                    ("Medium blur", Some(200.0)),
                    ("Heavy blur", Some(500.0)),
                ] {
                    let state = self.state.clone();
                    let color = background.clone();
                    let active = match (preset, blur_intensity) {
                        (None, None) => true,
                        (Some(preset), Some(current)) => (preset - current).abs() < 0.5,
                        _ => false,
                    };
                    blur_row = blur_row.child(
                        Button::new(eid("backdrop", label), label)
                            .variant(if active {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Outline
                            })
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| {
                                let background = match preset {
                                    None => Background::Color { color: color.clone() },
                                    Some(blur_intensity) => Background::Blur { blur_intensity },
                                };
                                state.update(cx, |state, cx| {
                                    state.execute(
                                        Command::UpdateSettings(SettingsPatch {
                                            background: Some(background),
                                            ..Default::default()
                                        }),
                                        cx,
                                    );
                                })
                            }),
                    );
                }
                content = content
                    .child(self.heading("Project", window))
                    .child(name_row)
                    .child(background_row)
                    .child(blur_row);

                let mut fps_row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .child(Self::label_cell("Frame rate", window));
                for (label, option) in [
                    ("24", FrameRate::FPS_24),
                    ("25", FrameRate::FPS_25),
                    ("30", FrameRate::FPS_30),
                    ("50", FrameRate::FPS_50),
                    ("60", FrameRate::FPS_60),
                ] {
                    let state = self.state.clone();
                    let active = rate == option;
                    fps_row = fps_row.child(
                        Button::new(eid("fps", label), label)
                            .variant(if active {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Outline
                            })
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    state.execute(
                                        Command::UpdateSettings(SettingsPatch {
                                            fps: Some(option),
                                            ..Default::default()
                                        }),
                                        cx,
                                    );
                                })
                            }),
                    );
                }
                let mut size_row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .child(Self::label_cell("Canvas", window));
                for (label, width, height) in [
                    ("1080p", 1920, 1080),
                    ("720p", 1280, 720),
                    ("4K", 3840, 2160),
                    ("Vertical", 1080, 1920),
                    ("Square", 1080, 1080),
                ] {
                    let state = self.state.clone();
                    let active = canvas == CanvasSize { width, height };
                    size_row = size_row.child(
                        Button::new(eid("canvas", label), label)
                            .variant(if active {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Outline
                            })
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    state.execute(
                                        Command::UpdateSettings(SettingsPatch {
                                            canvas: Some(CanvasSize { width, height }),
                                            ..Default::default()
                                        }),
                                        cx,
                                    );
                                })
                            }),
                    );
                }
                content = content.child(fps_row).child(size_row).child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Select a clip on the timeline to edit it."),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.card)
            .text_color(colors.card_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.))
                    .px(px(12.))
                    .border_b_1()
                    .border_color(colors.border)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Inspector"),
            )
            .child(content)
    }
}

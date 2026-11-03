use std::collections::HashMap;

use editor::{
    Background, CanvasSize, Command, Effect, ElementKind, ElementPatch, Mask, MaskSize, Params, SettingsPatch,
};
use gpui::{Context, Entity, FocusHandle, FontWeight, ScrollHandle, Window, div, prelude::*, px};
use time::FrameRate;

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
}

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

                if is_visual {
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
                }

                if is_visual {
                    content = content
                        .child(Separator::horizontal())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(self.heading("Effects", window))
                                .child({
                                    let state = self.state.clone();
                                    Button::new("add-blur", "Add blur")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(move |_, _, cx| {
                                            state.update(cx, |state, cx| state.add_effect(Effect::blur(), cx))
                                        })
                                }),
                        );
                    if element.effects.is_empty() {
                        content = content.child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("No effects. Blur softens the whole element; its strength can be keyframed."),
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
                        if effect.kind == Effect::BLUR {
                            let path = format!("effects.{effect_id}.params.intensity");
                            let row = self.row(
                                "Intensity",
                                &format!("effect.{effect_id}.intensity"),
                                "15",
                                number_text(effect_params.number("intensity", 15.0)),
                                Some(key_state(&path)),
                                window,
                                cx,
                                Box::new(move |text, state, cx| {
                                    if let Ok(value) = text.trim().parse::<f64>() {
                                        let mut params = Params::new();
                                        params.set("intensity", value.clamp(0.0, 100.0));
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
                                mask.params.text("strokeColor", "#ffffff").into_owned(),
                                None,
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

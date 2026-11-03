use std::collections::HashMap;

use editor::{Background, CanvasSize, Command, ElementKind, ElementPatch, Params, SettingsPatch};
use gpui::{Context, Entity, FontWeight, Window, div, prelude::*, px};
use time::FrameRate;

use crate::components::{Button, ButtonSize, ButtonVariant, Separator, TextInput, eid};
use crate::format::timecode;
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// Properties of the selected element, or of the project when nothing is
/// selected. Every field is a text input that commits on Enter or blur.
pub(crate) struct InspectorPanel {
    state: Entity<EditorState>,
    fields: HashMap<&'static str, Entity<TextInput>>,
}

/// Numeric element parameters shown under "Transform" and "Audio".
const NUMBER_PARAMS: &[(&str, &str, f64)] = &[
    ("transform.positionX", "Position X", 0.0),
    ("transform.positionY", "Position Y", 0.0),
    ("transform.scaleX", "Scale X", 1.0),
    ("transform.scaleY", "Scale Y", 1.0),
    ("transform.rotate", "Rotation", 0.0),
    ("opacity", "Opacity", 1.0),
];

const TEXT_NUMBER_PARAMS: &[(&str, &str, f64)] = &[
    ("fontSize", "Font size", 72.0),
    ("lineHeight", "Line height", 1.2),
    ("letterSpacing", "Letter spacing", 0.0),
];

const TEXT_STRING_PARAMS: &[(&str, &str, &str)] = &[
    ("content", "Text", "Default text"),
    ("fontFamily", "Font", "Arial"),
    ("color", "Color", "#ffffff"),
];

impl InspectorPanel {
    pub(crate) fn new(state: Entity<EditorState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let mut fields = HashMap::new();

        for (key, _, _) in NUMBER_PARAMS.iter().chain(TEXT_NUMBER_PARAMS) {
            let param = *key;
            let state_for_field = state.clone();
            fields.insert(
                param,
                cx.new(|cx| {
                    TextInput::new(window, cx, "").on_commit(move |text, _, cx| {
                        if let Ok(value) = text.trim().parse::<f64>() {
                            let mut params = Params::new();
                            params.set(param, value);
                            state_for_field.update(cx, |state, cx| state.update_selected_params(params, cx));
                        }
                    })
                }),
            );
        }
        for (key, _, _) in TEXT_STRING_PARAMS {
            let param = *key;
            let state_for_field = state.clone();
            fields.insert(
                param,
                cx.new(|cx| {
                    TextInput::new(window, cx, "").on_commit(move |text, _, cx| {
                        let mut params = Params::new();
                        params.set(param, text.to_owned());
                        state_for_field.update(cx, |state, cx| state.update_selected_params(params, cx));
                    })
                }),
            );
        }
        {
            let state_for_field = state.clone();
            fields.insert(
                "volume",
                cx.new(|cx| {
                    TextInput::new(window, cx, "0").on_commit(move |text, _, cx| {
                        if let Ok(value) = text.trim().parse::<f64>() {
                            let mut params = Params::new();
                            params.set("volume", value.clamp(-60.0, 20.0));
                            state_for_field.update(cx, |state, cx| state.update_selected_params(params, cx));
                        }
                    })
                }),
            );
        }
        {
            let state_for_field = state.clone();
            fields.insert(
                "element.name",
                cx.new(|cx| {
                    TextInput::new(window, cx, "Name").on_commit(move |text, _, cx| {
                        let name = text.trim().to_owned();
                        if !name.is_empty() {
                            state_for_field.update(cx, |state, cx| {
                                state.patch_selected(
                                    ElementPatch {
                                        name: Some(name),
                                        ..Default::default()
                                    },
                                    cx,
                                )
                            });
                        }
                    })
                }),
            );
        }
        {
            let state_for_field = state.clone();
            fields.insert(
                "project.name",
                cx.new(|cx| {
                    TextInput::new(window, cx, "Project name").on_commit(move |text, _, cx| {
                        state_for_field.update(cx, |state, cx| state.rename_project(text.to_owned(), cx));
                    })
                }),
            );
        }
        {
            let state_for_field = state.clone();
            fields.insert(
                "project.background",
                cx.new(|cx| {
                    TextInput::new(window, cx, "#000000").on_commit(move |text, _, cx| {
                        if editor::parse_hex_color(text).is_some() {
                            let color = text.trim().to_owned();
                            state_for_field.update(cx, |state, cx| {
                                state.execute(
                                    Command::UpdateSettings(SettingsPatch {
                                        background: Some(Background::Color { color }),
                                        ..Default::default()
                                    }),
                                    cx,
                                );
                            });
                        }
                    })
                }),
            );
        }

        Self { state, fields }
    }

    fn sync_field(&self, key: &str, value: String, window: &Window, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get(key) {
            field.update(cx, |field, cx| field.set_text(value, window, cx));
        }
    }

    fn field(&self, key: &str) -> Entity<TextInput> {
        self.fields[key].clone()
    }

    fn row(
        &self,
        label: &'static str,
        key: &str,
        window: &Window,
    ) -> impl IntoElement {
        let colors = window.theme().colors;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .w(px(88.))
                    .flex_none()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(div().flex_1().child(self.field(key)))
    }

    fn heading(&self, text: &'static str, window: &Window) -> impl IntoElement {
        let colors = window.theme().colors;
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.foreground)
            .child(text)
    }
}

fn number_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{}", (value * 1000.0).round() / 1000.0)
    }
}

impl Render for InspectorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;
        let state = self.state.read(cx);
        let project = state.project();
        let rate = project.settings.fps;
        let selected = state.selected_element().cloned();
        let selection_count = state.selection.len();
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
        let background = match &project.settings.background {
            Background::Color { color } => color.clone(),
            Background::Blur { .. } => "#000000".to_owned(),
        };
        let canvas = project.settings.canvas;

        // Push the current values into the fields (skipped while typing).
        match &selected {
            Some(element) => {
                self.sync_field("element.name", element.name.clone(), window, cx);
                for (key, _, default) in NUMBER_PARAMS.iter().chain(TEXT_NUMBER_PARAMS) {
                    self.sync_field(key, number_text(element.params.number(key, *default)), window, cx);
                }
                for (key, _, default) in TEXT_STRING_PARAMS {
                    self.sync_field(key, element.params.text(key, default).into_owned(), window, cx);
                }
                self.sync_field("volume", number_text(element.params.number("volume", 0.0)), window, cx);
            }
            None => {
                self.sync_field("project.name", project_name.clone(), window, cx);
                self.sync_field("project.background", background.clone(), window, cx);
            }
        }

        let mut content = div()
            .id("inspector-content")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .p(px(12.))
            .gap(px(10.));

        match selected {
            Some(element) => {
                let kind = element.kind.label();
                let is_text = matches!(element.kind, ElementKind::Text);
                let has_audio = matches!(element.kind, ElementKind::Audio { .. })
                    || (matches!(element.kind, ElementKind::Video { .. }) && media_has_audio);
                let is_visual = element.kind.is_visual();
                let muted = element.params.bool("muted", false);
                let hidden = element.hidden;
                let align = element.params.text("textAlign", "center").into_owned();
                let bold = element.params.text("fontWeight", "normal").as_ref() == "bold";
                let italic = element.params.text("fontStyle", "normal").as_ref() == "italic";

                content = content
                    .child(self.heading("Element", window))
                    .child(self.row("Name", "element.name", window))
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
                            )),
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
                    for (key, label, _) in NUMBER_PARAMS {
                        content = content.child(self.row(label, key, window));
                    }
                }

                if is_text {
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Text", window));
                    for (key, label, _) in TEXT_STRING_PARAMS {
                        content = content.child(self.row(label, key, window));
                    }
                    for (key, label, _) in TEXT_NUMBER_PARAMS {
                        content = content.child(self.row(label, key, window));
                    }
                    let mut align_row = div().flex().items_center().gap(px(6.)).child(
                        div()
                            .w(px(88.))
                            .flex_none()
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child("Align"),
                    );
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
                        .child(
                            div()
                                .w(px(88.))
                                .flex_none()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("Style"),
                        )
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
                    content = content
                        .child(Separator::horizontal())
                        .child(self.heading("Audio", window))
                        .child(self.row("Volume (dB)", "volume", window))
                        .child({
                            let state = self.state.clone();
                            div().flex().items_center().gap(px(8.)).child(
                                div()
                                    .w(px(88.))
                                    .flex_none()
                                    .text_xs()
                                    .text_color(colors.muted_foreground)
                                    .child("Mute"),
                            )
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
                content = content
                    .child(self.heading("Project", window))
                    .child(self.row("Name", "project.name", window))
                    .child(self.row("Background", "project.background", window));

                let mut fps_row = div().flex().flex_wrap().items_center().gap(px(6.)).child(
                    div()
                        .w(px(88.))
                        .flex_none()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Frame rate"),
                );
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
                let mut size_row = div().flex().flex_wrap().items_center().gap(px(6.)).child(
                    div()
                        .w(px(88.))
                        .flex_none()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Canvas"),
                );
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

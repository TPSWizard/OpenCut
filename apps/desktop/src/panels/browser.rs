use editor::MediaKind;
use gpui::{
    ClickEvent, Context, Entity, FontWeight, ObjectFit, PathPromptOptions, PromptLevel, Window,
    div, img, prelude::*, px,
};

use crate::components::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, eid};
use crate::format::short_duration;
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// The media bin: import files, see what the project has, put it on the timeline.
pub(crate) struct MediaPanel {
    state: Entity<EditorState>,
}

impl MediaPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }

    /// Opens the file picker and imports whatever is chosen.
    pub(crate) fn import(state: &Entity<EditorState>, cx: &mut gpui::App) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import".into()),
        });
        let state = state.clone();
        cx.spawn(async move |cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                state
                    .update(cx, |state, cx| state.import_paths(paths, cx))
                    .ok();
            }
        })
        .detach();
    }

    fn remove(&mut self, media_id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Remove \"{name}\" from the project?"),
            Some("Clips that use it are removed from the timeline. The file itself is not touched."),
            &["Remove", "Cancel"],
            cx,
        );
        let state = self.state.clone();
        cx.spawn(async move |_, cx| {
            if answer.await == Ok(0) {
                state
                    .update(cx, |state, cx| state.remove_media(&media_id, cx))
                    .ok();
            }
        })
        .detach();
    }
}

impl Render for MediaPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;
        let state = self.state.read(cx);
        let project = state.project();
        let has_ffmpeg = state.has_ffmpeg();
        let focus_handle = state.focus_handle.clone();

        let rows: Vec<_> = project
            .media
            .iter()
            .map(|media| {
                let id = media.id.clone();
                let add_id = id.clone();
                let remove_id = id.clone();
                let remove_name = media.name.clone();
                let thumbnail = state.thumbnails.get(&media.id).cloned();
                let kind = match media.kind {
                    MediaKind::Video => "video",
                    MediaKind::Image => "image",
                    MediaKind::Audio => "audio",
                };
                let detail = match media.kind {
                    MediaKind::Image => format!(
                        "{}×{}",
                        media.width.unwrap_or(0),
                        media.height.unwrap_or(0)
                    ),
                    _ => media
                        .duration
                        .map(|d| short_duration(d.to_seconds_f64()))
                        .unwrap_or_default(),
                };
                let state_for_click = self.state.clone();
                let focus_for_click = focus_handle.clone();
                div()
                    .id(eid("media", &id))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(6.))
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.sidebar_accent))
                    .on_click(move |event: &ClickEvent, window, cx| {
                        window.focus(&focus_for_click);
                        if event.click_count() >= 2 {
                            state_for_click
                                .update(cx, |state, cx| state.add_media_to_timeline(&add_id, cx));
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .w(px(56.))
                            .h(px(32.))
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .bg(colors.muted)
                            .overflow_hidden()
                            .map(|this| match thumbnail {
                                Some(image) => this.child(
                                    img(image).size_full().object_fit(ObjectFit::Cover),
                                ),
                                None => this.child(
                                    div()
                                        .text_xs()
                                        .text_color(colors.muted_foreground)
                                        .child(kind),
                                ),
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(media.name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(Badge::new(kind).variant(BadgeVariant::Outline))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(colors.muted_foreground)
                                            .child(detail),
                                    ),
                            ),
                    )
                    .child({
                        let state = self.state.clone();
                        Button::new(eid("add", &id), "Add")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, window, cx| {
                                let state_for_focus = state.read(cx).focus_handle.clone();
                                window.focus(&state_for_focus);
                                let media_id = id.clone();
                                state.update(cx, |state, cx| state.add_media_to_timeline(&media_id, cx));
                            })
                    })
                    .child(
                        Button::new(eid("remove", &remove_id), "×")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconXSmall)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove(remove_id.clone(), remove_name.clone(), window, cx);
                            })),
                    )
            })
            .collect();

        let state_for_import = self.state.clone();
        let state_for_text = self.state.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.sidebar)
            .text_color(colors.sidebar_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .h(px(40.))
                    .border_b_1()
                    .border_color(colors.sidebar_border)
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Media"),
                    )
                    .child(
                        Button::new("add-text", "Text")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| {
                                state_for_text.update(cx, |state, cx| state.add_text(cx));
                            }),
                    )
                    .child(
                        Button::new("import", "Import…")
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| Self::import(&state_for_import, cx)),
                    ),
            )
            .child(
                div()
                    .id("media-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(4.))
                    .when(rows.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(12.))
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(if has_ffmpeg {
                                    "Import video, audio or images, then double-click one (or press Add) to put it on the timeline at the playhead."
                                } else {
                                    "Import images to start. Install FFmpeg to import video and audio."
                                }),
                        )
                    })
                    .children(rows),
            )
    }
}

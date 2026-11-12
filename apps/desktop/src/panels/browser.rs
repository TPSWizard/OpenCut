use editor::MediaKind;
use gpui::{
    ClickEvent, Context, Entity, FontWeight, ObjectFit, PathPromptOptions, PromptLevel, Window,
    div, img, prelude::*, px,
};

use std::collections::HashMap;

use crate::components::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, TextInput, eid};
use crate::format::{short_duration, timecode};
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// The media bin: import files, see what the project has, put it on the timeline.
pub(crate) struct MediaPanel {
    state: Entity<EditorState>,
    /// Showing the saved markers instead of the media.
    show_markers: bool,
    /// Note fields by marker time (ticks).
    notes: HashMap<i64, Entity<TextInput>>,
}

impl MediaPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state, show_markers: false, notes: HashMap::new() }
    }

    /// The saved markers: colour (click to change), time (click to go
    /// there), note, remove.
    fn marker_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let state = self.state.read(cx);
        let rate = state.project().settings.fps;
        let editor_focus = state.focus_handle.clone();
        let markers = state.project().markers.clone();
        let times: Vec<i64> = markers.iter().map(|marker| marker.time.as_ticks()).collect();
        self.notes.retain(|time, _| times.contains(time));
        let mut rows = Vec::new();
        for marker in markers {
            let time = marker.time;
            let key = time.as_ticks();
            let field = match self.notes.get(&key) {
                Some(field) => field.clone(),
                None => {
                    let state = self.state.clone();
                    let focus = editor_focus.clone();
                    let field = cx.new(|cx| {
                        TextInput::new(window, cx, "Note")
                            .focus_after(focus)
                            .on_commit(move |text, _, cx| {
                                state.update(cx, |state, cx| state.set_marker_note(time, text, cx));
                            })
                    });
                    self.notes.insert(key, field.clone());
                    field
                }
            };
            let note = marker.note.clone().unwrap_or_default();
            field.update(cx, |field, cx| field.set_text(note, window, cx));
            let color = marker
                .color
                .as_deref()
                .and_then(|text| text.strip_prefix('#'))
                .and_then(|text| u32::from_str_radix(text, 16).ok())
                .unwrap_or(0x009dff);
            let seek_state = self.state.clone();
            let color_state = self.state.clone();
            let remove_state = self.state.clone();
            let length = marker
                .duration
                .map(|length| format!(" +{}", short_duration(length.to_seconds_f64())))
                .unwrap_or_default();
            let id = key.to_string();
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .p(px(4.))
                    .child(
                        div()
                            .id(eid("marker-color", &id))
                            .size(px(12.))
                            .rounded(px(2.))
                            .bg(gpui::rgb(color))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| color_state.update(cx, |state, cx| state.cycle_marker_color(time, cx))),
                    )
                    .child(
                        Button::new(eid("marker-seek", &id), format!("{}{length}", timecode(time, rate)))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| seek_state.update(cx, |state, cx| state.seek(time, cx))),
                    )
                    .child(div().flex_1().min_w_0().child(field))
                    .child(
                        Button::new(eid("marker-remove", &id), "×")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconXSmall)
                            .on_click(move |_, _, cx| remove_state.update(cx, |state, cx| state.remove_marker(time, cx))),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    /// Opens the file picker and imports whatever is chosen.
    /// Picks the file a missing media item should point at.
    fn relink(state: &Entity<EditorState>, media_id: String, cx: &mut gpui::App) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Relink".into()),
        });
        let state = state.clone();
        cx.spawn(async move |cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                state.update(cx, |state, cx| state.relink_media(&media_id, path, cx)).ok();
            }
        })
        .detach();
    }

    /// Picks a folder to look for every missing file in.
    fn find_missing(state: &Entity<EditorState>, cx: &mut gpui::App) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Look here".into()),
        });
        let state = state.clone();
        cx.spawn(async move |cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(dir) = paths.into_iter().next()
            {
                state.update(cx, |state, cx| state.relink_missing_in(dir, cx)).ok();
            }
        })
        .detach();
    }

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
        let marker_count = project.markers.len();
        let show_markers = self.show_markers;

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
                let proxy_media_id = id.clone();
                let relink_media_id = id.clone();
                let missing = state.missing_media.contains(&media.id);
                let is_video = media.kind == MediaKind::Video;
                let proxy_ready = state.proxies_ready.contains(&media.id);
                let proxy_making = state.proxies_making.contains(&media.id);
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
                                    .when(missing, |row| row.child(Badge::new("missing").variant(BadgeVariant::Destructive)))
                                    .when(proxy_ready, |row| row.child(Badge::new("proxy").variant(BadgeVariant::Secondary)))
                                    .when(proxy_making, |row| row.child(Badge::new("proxy…").variant(BadgeVariant::Outline)))
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
                    .when(missing, |row| {
                        let state = self.state.clone();
                        let media_id = relink_media_id.clone();
                        row.child(
                            Button::new(eid("relink", &relink_media_id), "Relink…")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| Self::relink(&state, media_id.clone(), cx)),
                        )
                    })
                    .when(is_video && has_ffmpeg && !proxy_making && !missing, |row| {
                        let state = self.state.clone();
                        let proxy_id = proxy_media_id.clone();
                        row.child(
                            Button::new(eid("proxy", &proxy_media_id), if proxy_ready { "Drop proxy" } else { "Proxy" })
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| {
                                        if proxy_ready {
                                            state.remove_proxy(&proxy_id, cx);
                                        } else {
                                            state.make_proxy(&proxy_id, cx);
                                        }
                                    });
                                }),
                        )
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
        let state_for_find = self.state.clone();
        let state_for_marker = self.state.clone();
        let state_for_chapters = self.state.clone();
        let missing_count = state.missing_media.len();
        let marker_rows = if show_markers { self.marker_rows(window, cx) } else { Vec::new() };
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
                        Button::new("tab-media", "Media")
                            .variant(if show_markers { ButtonVariant::Ghost } else { ButtonVariant::Secondary })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-markers", format!("Markers {marker_count}"))
                            .variant(if show_markers { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = true;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .when(show_markers, |row| {
                        row.child(
                            Button::new("copy-chapters", "Chapters")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| state_for_chapters.update(cx, |state, cx| state.copy_chapters(cx))),
                        )
                        .child(
                            Button::new("add-marker", "Add (M)")
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| {
                                    state_for_marker.update(cx, |state, cx| state.toggle_marker_at_playhead(cx))
                                }),
                        )
                    })
                    .when(missing_count > 0 && !show_markers, |row| {
                        row.child(
                            Button::new("find-missing", format!("Find {missing_count} missing…"))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| Self::find_missing(&state_for_find, cx)),
                        )
                    })
                    .when(!show_markers, |row| row.child(
                        Button::new("add-text", "Text")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| {
                                state_for_text.update(cx, |state, cx| state.add_text(cx));
                            }),
                    ))
                    .when(!show_markers, |row| row.child(
                        Button::new("import", "Import…")
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| Self::import(&state_for_import, cx)),
                    )),
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
                    .when(show_markers && marker_rows.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(12.))
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child("No markers yet. Press M to add one at the playhead. I and O mark a range; Ctrl+Shift+A selects the clips in it."),
                        )
                    })
                    .when(show_markers, |this| this.children(marker_rows))
                    .when(!show_markers && rows.is_empty(), |this| {
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
                    .when(!show_markers, |this| this.children(rows)),
            )
    }
}

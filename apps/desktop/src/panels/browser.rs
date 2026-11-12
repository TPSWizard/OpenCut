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
    /// Showing the sticker packs instead of the media.
    show_stickers: bool,
    /// 0 = Twemoji, 1 = Noto Emoji, 2 = Brands.
    sticker_pack: usize,
    sticker_query: String,
    skin_tone: usize,
    /// 0 = the brand's colour, 1 = white, 2 = black.
    brand_color: usize,
    emoji_category: usize,
    /// Emoji category lists by set, once fetched.
    emoji: HashMap<String, std::sync::Arc<crate::stickers::EmojiCollection>>,
    emoji_loading: Option<String>,
    sticker_search: Option<Entity<TextInput>>,
    /// Note fields by marker time (ticks).
    notes: HashMap<i64, Entity<TextInput>>,
}

impl MediaPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            show_markers: false,
            show_stickers: false,
            sticker_pack: 0,
            sticker_query: String::new(),
            skin_tone: 0,
            brand_color: 0,
            emoji_category: 0,
            emoji: HashMap::new(),
            emoji_loading: None,
            sticker_search: None,
            notes: HashMap::new(),
        }
    }

    /// Fetches an emoji set's category lists in the background.
    fn load_emoji(&mut self, set: &'static str, cx: &mut Context<Self>) {
        if self.emoji.contains_key(set) || self.emoji_loading.as_deref() == Some(set) {
            return;
        }
        self.emoji_loading = Some(set.to_owned());
        let dir = self.state.read(cx).stickers_dir.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { crate::stickers::load_emoji_collection(&dir, set) })
                .await;
            this.update(cx, |panel, cx| {
                panel.emoji_loading = None;
                match loaded {
                    Ok(collection) => {
                        panel.emoji.insert(set.to_owned(), std::sync::Arc::new(collection));
                    }
                    Err(message) => {
                        panel.state.update(cx, |state, cx| state.set_status(format!("Emoji list: {message}"), cx));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The sticker packs: pick a pack, search (Enter), click a name to put
    /// it on the timeline at the playhead.
    fn sticker_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        use crate::stickers::{EMOJI_SETS, SKIN_TONES, TRADEMARK_NOTICE};
        let colors = window.theme().colors;
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        let editor_focus = self.state.read(cx).focus_handle.clone();
        let search = match &self.sticker_search {
            Some(search) => search.clone(),
            None => {
                let this = cx.entity().clone();
                let search = cx.new(|cx| {
                    TextInput::new(window, cx, "Search, then Enter")
                        .focus_after(editor_focus)
                        .on_commit(move |text, _, cx| {
                            let text = text.to_owned();
                            this.update(cx, |panel, cx| {
                                panel.sticker_query = text;
                                cx.notify();
                            });
                        })
                });
                self.sticker_search = Some(search.clone());
                search
            }
        };
        let packs = ["Twemoji", "Noto Emoji", "Brands"];
        let mut pack_row = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
        for (index, label) in packs.iter().enumerate() {
            pack_row = pack_row.child(
                Button::new(eid("sticker-pack", label), *label)
                    .variant(if index == self.sticker_pack { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                    .size(ButtonSize::XSmall)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sticker_pack = index;
                        this.emoji_category = 0;
                        cx.notify();
                    })),
            );
        }
        rows.push(pack_row.into_any_element());
        rows.push(div().px(px(4.)).child(search).into_any_element());

        let query = self.sticker_query.trim().to_owned();
        if self.sticker_pack < 2 {
            let (set, _, credit) = EMOJI_SETS[self.sticker_pack];
            let mut tones = div().flex().items_center().gap(px(4.)).p(px(4.)).child(
                div().text_xs().text_color(colors.muted_foreground).child("Skin tone"),
            );
            for (index, (label, _)) in SKIN_TONES.iter().enumerate() {
                tones = tones.child(
                    Button::new(eid("skin-tone", label), *label)
                        .variant(if index == self.skin_tone { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                        .size(ButtonSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.skin_tone = index;
                            cx.notify();
                        })),
                );
            }
            rows.push(tones.into_any_element());
            rows.push(div().px(px(6.)).text_xs().text_color(colors.muted_foreground).child(credit).into_any_element());
            let Some(collection) = self.emoji.get(set).cloned() else {
                self.load_emoji(set, cx);
                rows.push(div().p(px(8.)).text_xs().text_color(colors.muted_foreground).child("Loading the emoji list…").into_any_element());
                return rows;
            };
            let names: Vec<String> = if query.is_empty() {
                let mut categories = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
                for (index, (category, _)) in collection.categories.iter().enumerate() {
                    categories = categories.child(
                        Button::new(eid("emoji-category", category), category.clone())
                            .variant(if index == self.emoji_category { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.emoji_category = index;
                                cx.notify();
                            })),
                    );
                }
                rows.push(categories.into_any_element());
                collection.categories.get(self.emoji_category).map(|(_, names)| names.iter().take(120).cloned().collect()).unwrap_or_default()
            } else {
                crate::stickers::search_emoji(&collection, &query, 120)
            };
            let suffix = SKIN_TONES[self.skin_tone].1;
            let mut grid = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
            for name in names {
                let toned = crate::stickers::with_skin_tone(&name, suffix, &collection.names);
                let label = name.replace('-', " ");
                let sticker_id = format!("icons:{set}:{toned}");
                let state = self.state.clone();
                let add_label = label.clone();
                grid = grid.child(
                    Button::new(eid("emoji", &sticker_id), label)
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::XSmall)
                        .on_click(move |_, _, cx| {
                            let (id, name) = (sticker_id.clone(), add_label.clone());
                            state.update(cx, |state, cx| state.add_sticker(id, name, cx));
                        }),
                );
            }
            rows.push(grid.into_any_element());
        } else {
            let mut colours = div().flex().items_center().gap(px(4.)).p(px(4.));
            for (index, label) in ["Brand colour", "White", "Black"].iter().enumerate() {
                colours = colours.child(
                    Button::new(eid("brand-colour", label), *label)
                        .variant(if index == self.brand_color { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                        .size(ButtonSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.brand_color = index;
                            cx.notify();
                        })),
                );
            }
            rows.push(colours.into_any_element());
            rows.push(
                div()
                    .mx(px(4.))
                    .p(px(6.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(colors.border)
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(TRADEMARK_NOTICE)
                    .into_any_element(),
            );
            let mut grid = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
            for brand in crate::stickers::search_brands(&query, 120) {
                let hex = match self.brand_color {
                    1 => "FFFFFF".to_owned(),
                    2 => "000000".to_owned(),
                    _ => brand.hex.clone(),
                };
                let swatch = u32::from_str_radix(&brand.hex, 16).unwrap_or(0x888888);
                let sticker_id = crate::stickers::brand_sticker_id(&brand.slug, &hex);
                let state = self.state.clone();
                let title = brand.title.clone();
                grid = grid.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(div().size(px(8.)).rounded_full().bg(gpui::rgb(swatch)))
                        .child(
                            Button::new(eid("brand", &sticker_id), brand.title.clone())
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| {
                                    let (id, name) = (sticker_id.clone(), title.clone());
                                    state.update(cx, |state, cx| state.add_sticker(id, name, cx));
                                }),
                        ),
                );
            }
            rows.push(grid.into_any_element());
        }
        rows
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
        let show_stickers = self.show_stickers;
        let show_media = !show_markers && !show_stickers;

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
        let sticker_rows = if show_stickers { self.sticker_rows(window, cx) } else { Vec::new() };
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
                            .variant(if show_media { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = false;
                                this.show_stickers = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-markers", format!("Markers {marker_count}"))
                            .variant(if show_markers { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = true;
                                this.show_stickers = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-stickers", "Stickers")
                            .variant(if show_stickers { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_stickers = true;
                                this.show_markers = false;
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
                    .when(missing_count > 0 && show_media, |row| {
                        row.child(
                            Button::new("find-missing", format!("Find {missing_count} missing…"))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| Self::find_missing(&state_for_find, cx)),
                        )
                    })
                    .when(show_media, |row| row.child(
                        Button::new("add-text", "Text")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| {
                                state_for_text.update(cx, |state, cx| state.add_text(cx));
                            }),
                    ))
                    .when(show_media, |row| row.child(
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
                    .when(show_stickers, |this| this.children(sticker_rows))
                    .when(show_media && rows.is_empty(), |this| {
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
                    .when(show_media, |this| this.children(rows)),
            )
    }
}

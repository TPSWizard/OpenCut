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
use crate::i18n::{t, tf};

/// The media bin: import files, see what the project has, put it on the timeline.
pub(crate) struct MediaPanel {
    state: Entity<EditorState>,
    /// Showing the saved markers instead of the media.
    show_markers: bool,
    /// Showing the sticker packs instead of the media.
    show_stickers: bool,
    /// Showing the voice-over tools instead of the media.
    show_voice: bool,
    /// Showing stock search instead of the media.
    show_stock: bool,
    stock_input: Option<Entity<TextInput>>,
    stock_query: String,
    stock_provider: &'static str,
    stock_kind: &'static str,
    stock_items: Vec<editor::StockItem>,
    stock_note: Option<String>,
    stock_busy: bool,
    voice_script: String,
    voice_input: Option<Entity<TextInput>>,
    /// The computer's voices, once listed (`None` while loading).
    voices: Option<Vec<String>>,
    voice: usize,
    voice_speed: f64,
    voice_captions: bool,
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
            show_voice: false,
            show_stock: false,
            stock_input: None,
            stock_query: String::new(),
            stock_provider: "openverse",
            stock_kind: "image",
            stock_items: Vec::new(),
            stock_note: None,
            stock_busy: false,
            voice_script: String::new(),
            voice_input: None,
            voices: None,
            voice: 0,
            voice_speed: 1.0,
            voice_captions: true,
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

    /// Runs the stock search in the background.
    fn search_stock(&mut self, cx: &mut Context<Self>) {
        let query = self.stock_query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        let (provider, kind) = (self.stock_provider, self.stock_kind);
        self.stock_busy = true;
        self.stock_note = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { crate::stock::search(provider, kind, &query, 1) }).await;
            this.update(cx, |panel, cx| {
                panel.stock_busy = false;
                match found {
                    Ok(items) => {
                        if items.is_empty() {
                            panel.stock_note = Some(t("Nothing found.").to_owned());
                        }
                        panel.stock_items = items;
                    }
                    Err(message) => {
                        panel.stock_items.clear();
                        panel.stock_note = Some(message);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The stock tab: search, source, pictures or videos, results with
    /// licence and creator, Add, and the project's credits.
    fn stock_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let colors = window.theme().colors;
        let editor_focus = self.state.read(cx).focus_handle.clone();
        let input = match &self.stock_input {
            Some(input) => input.clone(),
            None => {
                let this = cx.entity().clone();
                let input = cx.new(|cx| {
                    TextInput::new(window, cx, t("Search pictures and videos, then Enter"))
                        .focus_after(editor_focus)
                        .on_commit(move |text, _, cx| {
                            let text = text.to_owned();
                            this.update(cx, |panel, cx| {
                                panel.stock_query = text;
                                panel.search_stock(cx);
                            });
                        })
                });
                self.stock_input = Some(input.clone());
                input
            }
        };
        let mut rows: Vec<gpui::AnyElement> = vec![div().p(px(4.)).child(input).into_any_element()];
        let mut choices = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
        for (id, label, provider) in [("stock-openverse", "Openverse", "openverse"), ("stock-pixabay", "Pixabay", "pixabay")] {
            choices = choices.child(
                Button::new(id, label)
                    .variant(if self.stock_provider == provider { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stock_provider = provider;
                        if provider == "openverse" {
                            this.stock_kind = "image";
                        }
                        this.search_stock(cx);
                        cx.notify();
                    })),
            );
        }
        for (id, label, kind) in [("stock-images", t("Pictures"), "image"), ("stock-videos", t("Videos"), "video")] {
            choices = choices.child(
                Button::new(id, label)
                    .variant(if self.stock_kind == kind { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stock_kind = kind;
                        this.search_stock(cx);
                        cx.notify();
                    })),
            );
        }
        let credits_state = self.state.clone();
        choices = choices.child(div().flex_1()).child(
            Button::new("stock-credits", t("Copy credits"))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::XSmall)
                .on_click(move |_, _, cx| credits_state.update(cx, |state, cx| state.copy_stock_credits(cx))),
        );
        rows.push(choices.into_any_element());
        rows.push(
            div()
                .px(px(4.))
                .text_xs()
                .text_color(colors.muted_foreground)
                .child(if self.stock_provider == "openverse" {
                    t("Pictures from Openverse that are free to use and change, even commercially (CC0, public domain, CC BY). Most need a credit: it is kept with each one.")
                } else {
                    t("Pictures and videos from Pixabay, under the Pixabay Content License. The credit is kept with each one.")
                })
                .into_any_element(),
        );
        if self.stock_busy {
            rows.push(div().p(px(4.)).text_xs().child(t("Searching…")).into_any_element());
        }
        if let Some(note) = &self.stock_note {
            rows.push(div().p(px(4.)).text_xs().text_color(colors.muted_foreground).child(note.clone()).into_any_element());
        }
        for (index, item) in self.stock_items.iter().enumerate() {
            let state = self.state.clone();
            let chosen = item.clone();
            let detail = match item.duration {
                Some(seconds) => format!("{} · {} · {} s", item.creator, item.license, seconds.round()),
                None => format!("{} · {} · {}×{}", item.creator, item.license, item.width, item.height),
            };
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(4.))
                    .py(px(2.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_xs().truncate().child(item.title.clone()))
                            .child(div().text_xs().text_color(colors.muted_foreground).truncate().child(detail)),
                    )
                    .child(
                        Button::new(eid("stock-add", &index.to_string()), t("Add"))
                            .size(ButtonSize::XSmall)
                            .on_click(move |_, _, cx| {
                                let chosen = chosen.clone();
                                state.update(cx, |state, cx| state.import_stock(chosen, cx));
                            }),
                    )
                    .into_any_element(),
            );
        }
        if !self.stock_items.is_empty() {
            rows.push(
                div()
                    .px(px(4.))
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(if self.stock_provider == "openverse" {
                        t("Results from Openverse (openverse.org).")
                    } else {
                        t("Results from Pixabay (pixabay.com).")
                    })
                    .into_any_element(),
            );
        }
        rows
    }

    /// Lists the computer's voices in the background (once).
    fn load_voices(&mut self, cx: &mut Context<Self>) {
        if self.voices.is_some() {
            return;
        }
        self.voices = Some(Vec::new());
        cx.spawn(async move |this, cx| {
            let voices = cx.background_executor().spawn(async move { crate::voiceover::system_voices() }).await;
            this.update(cx, |panel, cx| {
                panel.voices = Some(voices);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The voice-over tab: a script (typed, or read from a text file),
    /// voice, speed, captions, and Speak.
    fn voice_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let colors = window.theme().colors;
        let editor_focus = self.state.read(cx).focus_handle.clone();
        let input = match &self.voice_input {
            Some(input) => input.clone(),
            None => {
                let this = cx.entity().clone();
                let input = cx.new(|cx| {
                    TextInput::new(window, cx, t("Script (sentences), then Enter"))
                        .focus_after(editor_focus)
                        .on_commit(move |text, _, cx| {
                            let text = text.to_owned();
                            this.update(cx, |panel, cx| {
                                panel.voice_script = text;
                                cx.notify();
                            });
                        })
                });
                self.voice_input = Some(input.clone());
                input
            }
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        rows.push(div().p(px(4.)).child(input.clone()).into_any_element());
        let lines = editor::split_script(&self.voice_script, editor::MAX_LINE_CHARS).len();
        rows.push(
            div()
                .px(px(4.))
                .text_xs()
                .text_color(colors.muted_foreground)
                .child(if lines == 0 {
                    t("Type the script and press Enter, or read it from a text file. Each sentence becomes a line.").to_owned()
                } else {
                    tf("{count} line(s) ready.", &[("count", &lines)])
                })
                .into_any_element(),
        );
        let from_file = {
            let this = cx.entity().clone();
            Button::new("voice-file", t("From text file…"))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::XSmall)
                .on_click(move |_, window, cx| {
                    let this = this.clone();
                    let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: false,
                        prompt: Some(t("Script").into()),
                    });
                    let _ = window;
                    cx.spawn(async move |cx| {
                        let Ok(Ok(Some(paths))) = receiver.await else { return };
                        let Some(path) = paths.into_iter().next() else { return };
                        let Ok(text) = std::fs::read_to_string(&path) else { return };
                        this.update(cx, |panel, cx| {
                            panel.voice_script = text;
                            cx.notify();
                        })
                        .ok();
                    })
                    .detach();
                })
        };
        rows.push(div().p(px(4.)).flex().gap(px(4.)).child(from_file).into_any_element());
        let voices = self.voices.clone().unwrap_or_default();
        let mut voice_row = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
        if voices.is_empty() {
            voice_row = voice_row.child(
                div().text_xs().text_color(colors.muted_foreground).child(if self.voices.is_none() {
                    t("Listing voices…")
                } else {
                    t("No system voices found (Windows speech, macOS say, or espeak-ng on Linux).")
                }),
            );
        }
        for (index, name) in voices.iter().enumerate() {
            voice_row = voice_row.child(
                Button::new(eid("voice", &index.to_string()), name.clone())
                    .variant(if index == self.voice { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.voice = index;
                        cx.notify();
                    })),
            );
        }
        rows.push(voice_row.into_any_element());
        let mut speed_row = div().flex().flex_wrap().items_center().gap(px(4.)).p(px(4.)).child(
            div().text_xs().text_color(colors.muted_foreground).mr(px(4.)).child(t("Speed")),
        );
        for speed in [0.8, 0.9, 1.0, 1.1, 1.25] {
            speed_row = speed_row.child(
                Button::new(eid("voice-speed", &format!("{speed}")), format!("{speed}×"))
                    .variant(if (self.voice_speed - speed).abs() < 1e-6 { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.voice_speed = speed;
                        cx.notify();
                    })),
            );
        }
        let captions = self.voice_captions;
        speed_row = speed_row.child(
            Button::new("voice-captions", if captions { t("Captions: on") } else { t("Captions: off") })
                .variant(if captions { ButtonVariant::Default } else { ButtonVariant::Outline })
                .size(ButtonSize::XSmall)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.voice_captions = !captions;
                    cx.notify();
                })),
        );
        rows.push(speed_row.into_any_element());
        let state = self.state.clone();
        let script = self.voice_script.clone();
        let voice = voices.get(self.voice).cloned();
        let speed = self.voice_speed;
        rows.push(
            div()
                .p(px(4.))
                .child(
                    Button::new("voice-speak", t("Speak at the playhead"))
                        .size(ButtonSize::Small)
                        .disabled(lines == 0)
                        .on_click(move |_, _, cx| {
                            let script = script.clone();
                            let voice = voice.clone();
                            state.update(cx, |state, cx| state.voiceover(script, voice, speed, captions, cx));
                        }),
                )
                .into_any_element(),
        );
        rows.push(
            div()
                .px(px(4.))
                .text_xs()
                .text_color(colors.muted_foreground)
                .child(t("Uses this computer's own voices. The web editor speaks with Kokoro's natural voices instead."))
                .into_any_element(),
        );
        rows
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
                        panel.state.update(cx, |state, cx| state.set_status(tf("Emoji list: {error}", &[("error", &message)]), cx));
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
                    TextInput::new(window, cx, t("Search, then Enter"))
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
                Button::new(eid("sticker-pack", label), t(label))
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
                div().text_xs().text_color(colors.muted_foreground).child(t("Skin tone")),
            );
            for (index, (label, _)) in SKIN_TONES.iter().enumerate() {
                tones = tones.child(
                    Button::new(eid("skin-tone", label), t(label))
                        .variant(if index == self.skin_tone { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                        .size(ButtonSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.skin_tone = index;
                            cx.notify();
                        })),
                );
            }
            rows.push(tones.into_any_element());
            rows.push(div().px(px(6.)).text_xs().text_color(colors.muted_foreground).child(t(credit)).into_any_element());
            let Some(collection) = self.emoji.get(set).cloned() else {
                self.load_emoji(set, cx);
                rows.push(div().p(px(8.)).text_xs().text_color(colors.muted_foreground).child(t("Loading the emoji list…")).into_any_element());
                return rows;
            };
            let names: Vec<String> = if query.is_empty() {
                let mut categories = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
                for (index, (category, _)) in collection.categories.iter().enumerate() {
                    categories = categories.child(
                        Button::new(eid("emoji-category", category), emoji_category_label(category))
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
                    Button::new(eid("brand-colour", label), t(label))
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
                    .child(t(TRADEMARK_NOTICE))
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
                        TextInput::new(window, cx, t("Note"))
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
            prompt: Some(t("Relink").into()),
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
            prompt: Some(t("Look here").into()),
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
            prompt: Some(t("Import").into()),
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
            &tf("Remove {name} from the project?", &[("name", &format!("\"{name}\""))]),
            Some(t("Clips that use it are removed from the timeline. The file itself is not touched.")),
            &[t("Remove"), t("Cancel")],
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
        let show_voice = self.show_voice;
        let show_stock = self.show_stock;
        let show_media = !show_markers && !show_stickers && !show_voice && !show_stock;

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
                    MediaKind::Video => t("video"),
                    MediaKind::Image => t("image"),
                    MediaKind::Audio => t("audio"),
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
                                    .when(missing, |row| row.child(Badge::new(t("missing")).variant(BadgeVariant::Destructive)))
                                    .when(proxy_ready, |row| row.child(Badge::new(t("proxy")).variant(BadgeVariant::Secondary)))
                                    .when(proxy_making, |row| row.child(Badge::new(t("proxy…")).variant(BadgeVariant::Outline)))
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
                        Button::new(eid("add", &id), t("Add"))
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
                            Button::new(eid("relink", &relink_media_id), t("Relink…"))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::XSmall)
                                .on_click(move |_, _, cx| Self::relink(&state, media_id.clone(), cx)),
                        )
                    })
                    .when(is_video && has_ffmpeg && !proxy_making && !missing, |row| {
                        let state = self.state.clone();
                        let proxy_id = proxy_media_id.clone();
                        row.child(
                            Button::new(eid("proxy", &proxy_media_id), if proxy_ready { t("Drop proxy") } else { t("Proxy") })
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
        let voice_rows = if show_voice { self.voice_rows(window, cx) } else { Vec::new() };
        let stock_rows = if show_stock { self.stock_rows(window, cx) } else { Vec::new() };
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
                        Button::new("tab-media", t("Media"))
                            .variant(if show_media { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = false;
                                this.show_stickers = false;
                                this.show_voice = false;
                                this.show_stock = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-markers", tf("Markers {count}", &[("count", &marker_count)]))
                            .variant(if show_markers { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_markers = true;
                                this.show_stickers = false;
                                this.show_voice = false;
                                this.show_stock = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-stickers", t("Stickers"))
                            .variant(if show_stickers { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_stickers = true;
                                this.show_markers = false;
                                this.show_voice = false;
                                this.show_stock = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-voice", t("Voice"))
                            .variant(if show_voice { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_voice = true;
                                this.show_stock = false;
                                this.show_markers = false;
                                this.show_stickers = false;
                                this.load_voices(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-stock", t("Stock"))
                            .variant(if show_stock { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                            .size(ButtonSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_stock = true;
                                this.show_voice = false;
                                this.show_markers = false;
                                this.show_stickers = false;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .when(show_markers, |row| {
                        row.child(
                            Button::new("copy-chapters", t("Chapters"))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| state_for_chapters.update(cx, |state, cx| state.copy_chapters(cx))),
                        )
                        .child(
                            Button::new("add-marker", t("Add (M)"))
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| {
                                    state_for_marker.update(cx, |state, cx| state.toggle_marker_at_playhead(cx))
                                }),
                        )
                    })
                    .when(missing_count > 0 && show_media, |row| {
                        row.child(
                            Button::new("find-missing", tf("Find {count} missing…", &[("count", &missing_count)]))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .on_click(move |_, _, cx| Self::find_missing(&state_for_find, cx)),
                        )
                    })
                    .when(show_media, |row| row.child(
                        Button::new("add-text", t("Text"))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Small)
                            .on_click(move |_, _, cx| {
                                state_for_text.update(cx, |state, cx| state.add_text(cx));
                            }),
                    ))
                    .when(show_media, |row| row.child(
                        Button::new("import", t("Import…"))
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
                                .child(t("No markers yet. Press M to add one at the playhead. I and O mark a range; Ctrl+Shift+A selects the clips in it.")),
                        )
                    })
                    .when(show_markers, |this| this.children(marker_rows))
                    .when(show_stickers, |this| this.children(sticker_rows))
                    .when(show_voice, |this| this.children(voice_rows))
                    .when(show_stock, |this| this.children(stock_rows))
                    .when(show_media && rows.is_empty(), |this| {
                        this.child(
                            div()
                                .p(px(12.))
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(if has_ffmpeg {
                                    t("Import video, audio or images, then double-click one (or press Add) to put it on the timeline at the playhead.")
                                } else {
                                    t("Import images to start. Install FFmpeg to import video and audio.")
                                }),
                        )
                    })
                    .when(show_media, |this| this.children(rows)),
            )
    }
}

/// The emoji category's name in the interface language (the data's English
/// name when there is no entry).
fn emoji_category_label(category: &str) -> gpui::SharedString {
    let label = match category {
        "Smileys & Emotion" => t("Smileys & Emotion"),
        "People & Body" => t("People & Body"),
        "Animals & Nature" => t("Animals & Nature"),
        "Food & Drink" => t("Food & Drink"),
        "Travel & Places" => t("Travel & Places"),
        "Activities" => t("Activities"),
        "Objects" => t("Objects"),
        "Symbols" => t("Symbols"),
        "Flags" => t("Flags"),
        other => return other.to_owned().into(),
    };
    label.into()
}

use std::path::PathBuf;

use gpui::{Context, Entity, FontWeight, Window, div, prelude::*, px};
use media::{EncodeFormat, EncodeQuality};

use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::render_thread::ExportSettings;
use crate::state::EditorState;
use crate::theme::ActiveTheme;

pub(crate) enum ExportEvent {
    Close,
}

impl gpui::EventEmitter<ExportEvent> for ExportDialog {}

pub(crate) struct ExportDialog {
    state: Entity<EditorState>,
    output: Option<PathBuf>,
    format: EncodeFormat,
    quality: EncodeQuality,
    include_audio: bool,
}

impl ExportDialog {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let name = state.read(cx).project().name.clone();
        let default_dir = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .map(|home| home.join("Videos"))
            .filter(|dir| dir.is_dir());
        let output = default_dir.map(|dir| dir.join(format!("{}.mp4", safe_file_name(&name))));
        Self {
            state,
            output,
            format: EncodeFormat::Mp4,
            quality: EncodeQuality::High,
            include_audio: true,
        }
    }

    fn choose_path(&mut self, cx: &mut Context<Self>) {
        let name = self.state.read(cx).project().name.clone();
        let suggested = format!("{}.{}", safe_file_name(&name), self.format.extension());
        let directory = self
            .output
            .as_ref()
            .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                this.update(cx, |dialog, cx| {
                    dialog.format = EncodeFormat::from_path(&path);
                    dialog.output = Some(path);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let Some(mut output) = self.output.clone() else {
            return;
        };
        if output.extension().is_none() {
            output.set_extension(self.format.extension());
        }
        let settings = ExportSettings {
            output,
            format: self.format,
            quality: self.quality,
            include_audio: self.include_audio,
        };
        self.state
            .update(cx, |state, cx| state.start_export(settings, cx));
    }
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "export".to_owned()
    } else {
        trimmed.to_owned()
    }
}

impl Render for ExportDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;
        let state = self.state.read(cx);
        let project = state.project();
        let canvas = project.settings.canvas;
        let fps = project.settings.fps.as_f64().unwrap_or(30.0);
        let duration = state.duration().to_seconds_f64();
        let running = state.export.as_ref().filter(|e| e.finished.is_none());
        let finished = state.export.as_ref().and_then(|e| e.finished.clone());
        let progress = state.export.as_ref().map(|e| e.progress()).unwrap_or(0.0);
        let frames = state
            .export
            .as_ref()
            .map(|e| (e.frames_done, e.frames_total))
            .unwrap_or((0, 0));
        let has_ffmpeg = state.has_ffmpeg();
        let output_text = self
            .output
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Choose where to save…".to_owned());

        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .w(px(460.))
            .p(px(20.))
            .rounded(theme.radius)
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow_lg()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Export"))
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(format!(
                        "{}×{} · {fps:.2} fps · {duration:.1} s",
                        canvas.width, canvas.height
                    )),
            );

        if let Some(result) = finished {
            card = card
                .child(match &result {
                    Ok(path) => div()
                        .text_sm()
                        .child(format!("Saved to {}", path.display())),
                    Err(error) => div()
                        .text_sm()
                        .text_color(colors.destructive)
                        .child(format!("Export failed: {error}")),
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .when(result.is_ok(), |this| {
                            let path = result.clone().unwrap();
                            this.child(
                                Button::new("reveal", "Show in folder")
                                    .variant(ButtonVariant::Outline)
                                    .on_click(move |_, _, cx| cx.reveal_path(&path)),
                            )
                        })
                        .child(Button::new("close", "Close").on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| state.dismiss_export(cx));
                            cx.emit(ExportEvent::Close);
                        }))),
                );
        } else if running.is_some() {
            card = card
                .child(
                    div()
                        .text_sm()
                        .child(format!("Rendering frame {} of {}…", frames.0, frames.1)),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(8.))
                        .rounded_full()
                        .bg(colors.muted)
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(progress.clamp(0.0, 1.0)))
                                .rounded_full()
                                .bg(colors.primary),
                        ),
                )
                .child(
                    div().flex().justify_end().child(
                        Button::new("cancel-export", "Cancel")
                            .variant(ButtonVariant::Outline)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |state, cx| state.cancel_export(cx));
                            })),
                    ),
                );
        } else {
            let mut format_row = div().flex().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child("Format"),
            );
            for (label, format) in [("MP4 (H.264)", EncodeFormat::Mp4), ("WebM (VP9)", EncodeFormat::Webm)] {
                let active = self.format == format;
                format_row = format_row.child(
                    Button::new(eid("format", label), label)
                        .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.format = format;
                            if let Some(output) = &mut this.output {
                                output.set_extension(format.extension());
                            }
                            cx.notify();
                        })),
                );
            }
            let mut quality_row = div().flex().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child("Quality"),
            );
            for (label, quality) in [
                ("Low", EncodeQuality::Low),
                ("Medium", EncodeQuality::Medium),
                ("High", EncodeQuality::High),
                ("Best", EncodeQuality::VeryHigh),
            ] {
                let active = self.quality == quality;
                quality_row = quality_row.child(
                    Button::new(eid("quality", label), label)
                        .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.quality = quality;
                            cx.notify();
                        })),
                );
            }
            let include_audio = self.include_audio;
            let audio_row = div().flex().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child("Audio"),
            )
            .child(
                Button::new("audio", if include_audio { "Included" } else { "Left out" })
                    .variant(if include_audio { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.include_audio = !this.include_audio;
                        cx.notify();
                    })),
            );
            card = card
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child("Save to"))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(output_text),
                        )
                        .child(
                            Button::new("choose", "Choose…")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .on_click(cx.listener(|this, _, _, cx| this.choose_path(cx))),
                        ),
                )
                .child(format_row)
                .child(quality_row)
                .child(audio_row)
                .when(!has_ffmpeg, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(colors.destructive)
                            .child("FFmpeg is not installed, so nothing can be encoded."),
                    )
                })
                .when(duration <= 0.0, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(colors.destructive)
                            .child("The timeline is empty."),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            Button::new("cancel", "Cancel")
                                .variant(ButtonVariant::Outline)
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(ExportEvent::Close))),
                        )
                        .child(
                            Button::new("start", "Export")
                                .disabled(self.output.is_none() || !has_ffmpeg || duration <= 0.0)
                                .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                        ),
                );
        }

        div()
            .id("export-overlay")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.5))
            .child(card)
    }
}

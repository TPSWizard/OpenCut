use std::path::PathBuf;

use editor::{
    CanvasSize, Command, PLATFORM_PRESETS, PlatformFix, PlatformPreset, PlatformSeverity, SettingsPatch, Verified,
    check_platform_export, describe_aspect, estimate_platform_file_bytes, format_bytes, platform_encode_settings,
    platform_preset,
};
use gpui::{Context, Entity, FontWeight, Window, div, prelude::*, px};
use media::{EncodeFormat, EncodeQuality, PlatformEncode};

use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::render_thread::ExportSettings;
use crate::state::EditorState;
use crate::theme::ActiveTheme;
use crate::i18n::{t, tf};

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
    /// Platform preset the export is for; forces MP4 with its encoder settings.
    platform: Option<&'static PlatformPreset>,
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
            platform: None,
        }
    }

    fn choose_platform(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        self.platform = id.and_then(platform_preset);
        if self.platform.is_some() && self.format != EncodeFormat::Mp4 {
            self.format = EncodeFormat::Mp4;
            if let Some(output) = &mut self.output {
                output.set_extension("mp4");
            }
        }
        cx.notify();
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
        let platform = self.platform.filter(|_| self.format == EncodeFormat::Mp4).map(|_| {
            let project = self.state.read(cx).project();
            let fps = project.settings.fps.as_f64().unwrap_or(30.0);
            let encode = platform_encode_settings(project.settings.canvas, fps);
            PlatformEncode {
                video_bitrate: encode.video_bitrate,
                audio_bitrate: encode.audio_bitrate,
                gop_frames: encode.gop_frames,
            }
        });
        let settings = ExportSettings {
            output,
            format: self.format,
            quality: self.quality,
            include_audio: self.include_audio,
            platform,
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
            .unwrap_or_else(|| t("Choose where to save…").to_owned());

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
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(t("Export")))
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
                        .child(tf("Saved to {path}", &[("path", &path.display())])),
                    Err(error) => div()
                        .text_sm()
                        .text_color(colors.destructive)
                        .child(tf("Export failed: {error}", &[("error", &error)])),
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .when(result.is_ok(), |this| {
                            let path = result.clone().unwrap();
                            this.child(
                                Button::new("reveal", t("Show in folder"))
                                    .variant(ButtonVariant::Outline)
                                    .on_click(move |_, _, cx| cx.reveal_path(&path)),
                            )
                        })
                        .child(Button::new("close", t("Close")).on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| state.dismiss_export(cx));
                            cx.emit(ExportEvent::Close);
                        }))),
                );
        } else if running.is_some() {
            card = card
                .child(
                    div()
                        .text_sm()
                        .child(tf("Rendering frame {done} of {total}…", &[("done", &frames.0), ("total", &frames.1)])),
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
                        Button::new("cancel-export", t("Cancel"))
                            .variant(ButtonVariant::Outline)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |state, cx| state.cancel_export(cx));
                            })),
                    ),
                );
        } else {
            let platform = self.platform;
            let mut platform_row = div().flex().flex_wrap().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Platform")),
            );
            let choices = std::iter::once(None).chain(PLATFORM_PRESETS.iter().map(Some));
            for choice in choices {
                let label = choice.map(|preset| preset.label).unwrap_or("Custom");
                let shown = if choice.is_some() { label } else { t("Custom") };
                let active = match (choice, platform) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.id == b.id,
                    _ => false,
                };
                let id = choice.map(|preset| preset.id);
                platform_row = platform_row.child(
                    Button::new(eid("platform", label), shown)
                        .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_platform(id, cx))),
                );
            }
            let mut platform_notes = div().flex().flex_col().gap(px(4.));
            if let Some(preset) = platform {
                let fps_value = project.settings.fps.as_f64().unwrap_or(30.0);
                let encode = platform_encode_settings(canvas, fps_value);
                let estimate = estimate_platform_file_bytes(canvas, fps_value, duration, self.include_audio);
                platform_notes = platform_notes.child(
                    div().text_xs().text_color(colors.muted_foreground).child(tf(
                        "MP4, H.264 High profile at {mbps} Mbps, closed GOP every {gop} frames, moov atom first, about {size}. Target {width}x{height} ({aspect}) at {fps} fps{limit}.",
                        &[
                            ("mbps", &(encode.video_bitrate as f64 / 1_000_000.0)),
                            ("gop", &encode.gop_frames),
                            ("size", &format_bytes(estimate)),
                            ("width", &preset.canvas.width),
                            ("height", &preset.canvas.height),
                            ("aspect", &describe_aspect(preset.canvas)),
                            ("fps", &preset.fps.as_f64().unwrap_or(30.0)),
                            (
                                "limit",
                                &preset
                                    .max_duration_seconds
                                    .map(|max| tf(", up to {minutes} min", &[("minutes", &(max / 60.0).round())]))
                                    .unwrap_or_default(),
                            ),
                        ],
                    )),
                );
                if preset.verified == Verified::ThirdParty {
                    platform_notes = platform_notes.child(
                        div().text_xs().text_color(colors.muted_foreground).child(tf(
                            "Limits come from third-party guides, not {platform}'s own pages.",
                            &[("platform", &preset.label)],
                        )),
                    );
                }
                let issues = check_platform_export(preset, canvas, project.settings.fps, duration, self.include_audio);
                if issues.is_empty() {
                    platform_notes = platform_notes
                        .child(div().text_xs().child(tf("The project matches {platform}.", &[("platform", &preset.label)])));
                }
                for issue in issues {
                    let color = match issue.severity {
                        PlatformSeverity::Error => colors.destructive,
                        PlatformSeverity::Warning => colors.foreground,
                    };
                    let mut row = div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_xs().text_color(color).child(issue.message.clone()));
                    if let Some(fix) = issue.fix {
                        let (label, shown, patch) = match fix {
                            PlatformFix::Canvas(CanvasSize { width, height }) => (
                                format!("Set canvas to {width}x{height}"),
                                tf("Set canvas to {width}x{height}", &[("width", &width), ("height", &height)]),
                                SettingsPatch { canvas: Some(CanvasSize { width, height }), ..Default::default() },
                            ),
                            PlatformFix::Fps(rate) => (
                                format!("Set project to {} fps", rate.as_f64().unwrap_or(30.0)),
                                tf("Set project to {fps} fps", &[("fps", &rate.as_f64().unwrap_or(30.0))]),
                                SettingsPatch { fps: Some(rate), ..Default::default() },
                            ),
                        };
                        let state = self.state.clone();
                        row = row.child(
                            div().child(
                                Button::new(eid("fix", &label), shown)
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Small)
                                    .on_click(move |_, _, cx| {
                                        let patch = patch.clone();
                                        state.update(cx, |state, cx| {
                                            state.execute(Command::UpdateSettings(patch), cx);
                                        });
                                    }),
                            ),
                        );
                    }
                    platform_notes = platform_notes.child(row);
                }
            }
            let mut format_row = div().flex().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Format")),
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
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Quality")),
            );
            for (label, quality) in [
                ("Low", EncodeQuality::Low),
                ("Medium", EncodeQuality::Medium),
                ("High", EncodeQuality::High),
                ("Best", EncodeQuality::VeryHigh),
            ] {
                let active = self.quality == quality;
                quality_row = quality_row.child(
                    Button::new(eid("quality", label), t(label))
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
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Audio")),
            )
            .child(
                Button::new("audio", if include_audio { t("Included") } else { t("Left out") })
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
                        .child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Save to")))
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
                            Button::new("choose", t("Choose…"))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .on_click(cx.listener(|this, _, _, cx| this.choose_path(cx))),
                        ),
                )
                .child(platform_row)
                .child(platform_notes)
                .when(platform.is_none(), |this| this.child(format_row).child(quality_row))
                .child(audio_row)
                .when(!has_ffmpeg, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(colors.destructive)
                            .child(t("FFmpeg is not installed, so nothing can be encoded.")),
                    )
                })
                .when(duration <= 0.0, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(colors.destructive)
                            .child(t("The timeline is empty.")),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            Button::new("cancel", t("Cancel"))
                                .variant(ButtonVariant::Outline)
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(ExportEvent::Close))),
                        )
                        .child(
                            Button::new("start", t("Export"))
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

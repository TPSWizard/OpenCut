use std::path::PathBuf;

use editor::{
    CanvasSize, Command, PLATFORM_PRESETS, PlatformFix, PlatformPreset, PlatformSeverity, SettingsPatch, Verified,
    check_platform_export, describe_aspect, estimate_platform_file_bytes, format_bytes, platform_encode_settings,
    platform_preset,
};
use gpui::{Context, Entity, FontWeight, Window, div, prelude::*, px};
use media::{AlphaCodec, EncodeFormat, EncodeQuality, PlatformEncode, VideoCodec, codecs_for, parse_bitrate_mbps, resolve_codec};

use crate::components::{Button, ButtonSize, ButtonVariant, TextInput, eid};
use crate::render_thread::{ExportKind, ExportSettings};
use crate::state::EditorState;
use crate::theme::ActiveTheme;
use crate::i18n::{t, tf};

pub(crate) enum ExportEvent {
    Close,
}

impl gpui::EventEmitter<ExportEvent> for ExportDialog {}

/// The kind of file the dialog writes (row 96).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Video,
    Gif,
    Audio,
    Still,
    Sequence,
}

pub(crate) struct ExportDialog {
    state: Entity<EditorState>,
    kind: Kind,
    gif_width: u32,
    gif_fps: u32,
    gif_dither: bool,
    audio_format: media::AudioFileFormat,
    /// Leave the background out and keep the alpha (row 97): videos,
    /// stills and image sequences.
    transparent: bool,
    /// The codec of a transparent video.
    alpha_codec: AlphaCodec,
    output: Option<PathBuf>,
    format: EncodeFormat,
    quality: EncodeQuality,
    include_audio: bool,
    /// Whole-mix loudness target id (`editor::LOUDNESS_TARGETS`); `None` leaves the mix as it is.
    loudness: Option<&'static str>,
    /// Platform preset the export is for; forces MP4 with its encoder settings.
    platform: Option<&'static PlatformPreset>,
    /// The codec asked for; the container's default when it does not take it.
    codec: Option<VideoCodec>,
    /// Codecs this FFmpeg can encode; `None` until it has been asked.
    available: Option<Vec<VideoCodec>>,
    hardware: bool,
    /// Encode at a target bitrate instead of `quality`.
    use_bitrate: bool,
    bitrate_text: String,
    bitrate_input: Option<Entity<TextInput>>,
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
        if let Some(ffmpeg) = state.read(cx).ffmpeg() {
            cx.spawn(async move |this, cx| {
                let codecs = cx.background_executor().spawn(async move { media::available_codecs(&ffmpeg) }).await;
                this.update(cx, |dialog, cx| {
                    dialog.available = Some(codecs);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        Self {
            state,
            kind: Kind::Video,
            gif_width: 480,
            gif_fps: 15,
            gif_dither: true,
            audio_format: media::AudioFileFormat::Aac,
            transparent: false,
            alpha_codec: AlphaCodec::ProRes4444,
            output,
            format: EncodeFormat::Mp4,
            quality: EncodeQuality::High,
            include_audio: true,
            loudness: None,
            platform: None,
            codec: None,
            available: None,
            hardware: false,
            use_bitrate: false,
            bitrate_text: "8".to_owned(),
            bitrate_input: None,
        }
    }

    fn chosen_codec(&self) -> VideoCodec {
        resolve_codec(self.format, self.codec)
    }

    fn video_bitrate(&self) -> Option<u64> {
        self.use_bitrate.then(|| parse_bitrate_mbps(&self.bitrate_text)).flatten()
    }

    fn codec_missing(&self) -> bool {
        self.platform.is_none()
            && self.available.as_ref().is_some_and(|codecs| !codecs.contains(&self.chosen_codec()))
    }

    /// The output's extension for the chosen kind (none for a sequence folder).
    fn extension(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Video if self.transparent => Some(self.alpha_codec.extension()),
            Kind::Video => Some(self.format.extension()),
            Kind::Gif => Some("gif"),
            Kind::Audio => Some(self.audio_format.extension()),
            Kind::Still => Some("png"),
            Kind::Sequence => None,
        }
    }

    fn choose_kind(&mut self, kind: Kind, cx: &mut Context<Self>) {
        self.kind = kind;
        let extension = self.extension();
        if let Some(output) = &mut self.output {
            match extension {
                Some(extension) => {
                    if output.extension().is_none() {
                        // A sequence folder becomes a file again.
                        let name = output.file_name().map(|name| name.to_string_lossy().trim_end_matches(" frames").to_owned()).unwrap_or_default();
                        output.set_file_name(name);
                    }
                    output.set_extension(extension);
                }
                None => {
                    let stem = output.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_else(|| "export".to_owned());
                    output.set_file_name(format!("{stem} frames"));
                }
            }
        }
        cx.notify();
    }

    /// Turns a transparent background on or off, or picks the transparent
    /// video's codec; the output's extension follows.
    fn choose_transparent(&mut self, transparent: bool, codec: Option<AlphaCodec>, cx: &mut Context<Self>) {
        self.transparent = transparent;
        if let Some(codec) = codec {
            self.alpha_codec = codec;
        }
        if transparent && self.kind == Kind::Video {
            self.platform = None;
        }
        if let (Some(extension), Some(output)) = (self.extension(), self.output.as_mut())
            && output.extension().is_some()
        {
            output.set_extension(extension);
        }
        cx.notify();
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
        let suggested = match self.extension() {
            Some(extension) => format!("{}.{extension}", safe_file_name(&name)),
            None => format!("{} frames", safe_file_name(&name)),
        };
        let directory = self
            .output
            .as_ref()
            .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                this.update(cx, |dialog, cx| {
                    let extension = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase());
                    match extension.as_deref() {
                        Some("gif") => dialog.kind = Kind::Gif,
                        Some("png") => dialog.kind = Kind::Still,
                        // ProRes 4444 is the one MOV this writes: a transparent video.
                        Some("mov") => {
                            dialog.kind = Kind::Video;
                            dialog.transparent = true;
                            dialog.alpha_codec = AlphaCodec::ProRes4444;
                            dialog.platform = None;
                        }
                        Some("webm") if dialog.transparent => {
                            dialog.kind = Kind::Video;
                            dialog.alpha_codec = AlphaCodec::Vp9;
                        }
                        Some("mp4" | "webm" | "mkv") => {
                            dialog.kind = Kind::Video;
                            dialog.transparent = false;
                            dialog.format = EncodeFormat::from_path(&path);
                        }
                        Some(_) => {
                            if let Some(format) = media::AudioFileFormat::from_path(&path) {
                                dialog.kind = Kind::Audio;
                                dialog.audio_format = format;
                            }
                        }
                        None => {}
                    }
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
        if let Some(extension) = self.extension()
            && output.extension().is_none()
        {
            output.set_extension(extension);
        }
        let kind = match self.kind {
            Kind::Video => ExportKind::Video,
            Kind::Gif => ExportKind::Gif { width: self.gif_width, fps: f64::from(self.gif_fps), dither: self.gif_dither },
            Kind::Audio => ExportKind::Audio(self.audio_format),
            Kind::Still => ExportKind::Still(self.state.read(cx).playhead()),
            Kind::Sequence => ExportKind::Sequence,
        };
        if self.kind != Kind::Video {
            let settings = ExportSettings {
                kind,
                output,
                format: self.format,
                quality: self.quality,
                include_audio: self.kind == Kind::Audio,
                platform: None,
                codec: None,
                video_bitrate: None,
                hardware: false,
                loudness: if self.kind == Kind::Audio { self.loudness.and_then(render::MixLoudness::target) } else { None },
                alpha: (self.transparent && matches!(self.kind, Kind::Still | Kind::Sequence)).then_some(self.alpha_codec),
            };
            self.state.update(cx, |state, cx| state.start_export(settings, cx));
            return;
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
        if self.transparent {
            // A transparent video: the codec and container follow from
            // `alpha_codec`; VP9 takes the quality or bitrate.
            if self.alpha_codec == AlphaCodec::Vp9 && self.use_bitrate && self.video_bitrate().is_none() {
                return;
            }
            let settings = ExportSettings {
                kind,
                output,
                format: EncodeFormat::Webm,
                quality: self.quality,
                include_audio: self.include_audio,
                platform: None,
                codec: None,
                video_bitrate: if self.alpha_codec == AlphaCodec::Vp9 { self.video_bitrate() } else { None },
                hardware: false,
                loudness: self.loudness.filter(|_| self.include_audio).and_then(render::MixLoudness::target),
                alpha: Some(self.alpha_codec),
            };
            self.state.update(cx, |state, cx| state.start_export(settings, cx));
            return;
        }
        if self.platform.is_none() && self.use_bitrate && self.video_bitrate().is_none() {
            return;
        }
        let settings = ExportSettings {
            kind,
            output,
            format: self.format,
            quality: self.quality,
            include_audio: self.include_audio,
            platform,
            codec: Some(self.chosen_codec()),
            video_bitrate: if platform.is_some() { None } else { self.video_bitrate() },
            hardware: platform.is_none() && self.hardware,
            loudness: self.loudness.filter(|_| self.include_audio).and_then(render::MixLoudness::target),
            alpha: None,
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
        let mastered = state.export.as_ref().and_then(|e| e.mastered);
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
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(div().text_sm().child(tf("Saved to {path}", &[("path", &path.display())])))
                        .children(mastered.map(|report| {
                            div().text_xs().text_color(colors.muted_foreground).child(format!(
                                "{} {:.1} → {:.1} LUFS ({:+.1} dB{}), {} {:.1} dBTP",
                                t("Mix loudness:"),
                                report.input_lufs,
                                report.output_lufs,
                                report.gain_db,
                                if report.limited { t(", peaks limited") } else { "" },
                                t("true peak"),
                                report.output_true_peak_dbtp
                            ))
                        })),
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
                        .child(if frames.1 == 0 {
                            t("Working…").to_owned()
                        } else {
                            tf("Rendering frame {done} of {total}…", &[("done", &frames.0), ("total", &frames.1)])
                        }),
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
            let kind = self.kind;
            let mut kind_row = div().flex().flex_wrap().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Kind")),
            );
            for (label, choice) in [
                ("Video", Kind::Video),
                ("GIF", Kind::Gif),
                ("Audio only", Kind::Audio),
                ("Still (PNG)", Kind::Still),
                ("Image sequence", Kind::Sequence),
            ] {
                kind_row = kind_row.child(
                    Button::new(eid("kind", label), t(label))
                        .variant(if kind == choice { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_kind(choice, cx))),
                );
            }
            let row = |label: &'static str| {
                div().flex().flex_wrap().items_center().gap(px(6.)).child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t(label)))
            };
            let mut gif_size_row = row("Size");
            for width in [320u32, 480, 640] {
                let (w, h) = render::gif_size(canvas.width, canvas.height, width);
                gif_size_row = gif_size_row.child(
                    Button::new(eid("gif-width", &width.to_string()), format!("{w}×{h}"))
                        .variant(if self.gif_width == width { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.gif_width = width;
                            cx.notify();
                        })),
                );
            }
            let mut gif_fps_row = row("Frame rate");
            for rate in [10u32, 15, 25] {
                gif_fps_row = gif_fps_row.child(
                    Button::new(eid("gif-fps", &rate.to_string()), format!("{rate} fps"))
                        .variant(if self.gif_fps == rate { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.gif_fps = rate;
                            cx.notify();
                        })),
                );
            }
            let gif_dither = self.gif_dither;
            let gif_dither_row = row("Dither").child(
                Button::new("gif-dither", if gif_dither { t("On") } else { t("Off") })
                    .variant(if gif_dither { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.gif_dither = !this.gif_dither;
                        cx.notify();
                    })),
            );
            let mut audio_format_row = row("Format");
            for (label, format) in [
                ("WAV", media::AudioFileFormat::Wav),
                ("AAC (.m4a)", media::AudioFileFormat::Aac),
                ("Opus", media::AudioFileFormat::Opus),
            ] {
                audio_format_row = audio_format_row.child(
                    Button::new(eid("audio-format", label), label)
                        .variant(if self.audio_format == format { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.audio_format = format;
                            if let Some(output) = &mut this.output {
                                output.set_extension(format.extension());
                            }
                            cx.notify();
                        })),
                );
            }
            let kind_note = match kind {
                Kind::Video => None,
                Kind::Gif => Some(t("Loops forever, up to 255 colours; only what changes is stored for each frame. No sound.").to_owned()),
                Kind::Audio => Some(t("The sound alone, mixed as in a video export.").to_owned()),
                Kind::Still => Some(tf(
                    "The frame at the playhead ({time} s) at {width}×{height}.",
                    &[("time", &format!("{:.2}", state.playhead().to_seconds_f64())), ("width", &canvas.width), ("height", &canvas.height)],
                )),
                Kind::Sequence => Some(tf(
                    "Every frame at {fps} fps as numbered PNGs (frame_000001.png, …) in a new folder.",
                    &[("fps", &format!("{fps:.2}"))],
                )),
            };
            // Background (row 97): as the project sets it, or left out with the alpha kept.
            let transparent = self.transparent;
            let mut background_row = row("Background");
            for (label, choice) in [("As set", false), ("Transparent", true)] {
                background_row = background_row.child(
                    Button::new(eid("background", label), t(label))
                        .variant(if transparent == choice { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_transparent(choice, None, cx))),
                );
            }
            let mut alpha_row = row("Format");
            for codec in AlphaCodec::ALL {
                alpha_row = alpha_row.child(
                    Button::new(eid("alpha-codec", codec.id()), codec.label())
                        .variant(if self.alpha_codec == codec { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_transparent(true, Some(codec), cx))),
                );
            }
            let alpha_note = match (kind, self.alpha_codec) {
                (Kind::Video, AlphaCodec::ProRes4444) => t("For other editors: Apple ProRes 4444 with its alpha channel, large files, PCM sound. The project's background is left out."),
                (Kind::Video, AlphaCodec::Vp9) => t("For the web: VP9 with an alpha channel, plays with its transparency in browsers. The project's background is left out."),
                _ => t("PNGs with an alpha channel: only the clips, nothing behind them."),
            };
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
                                        state.update(cx, |state, cx| match patch.canvas {
                                            // A new canvas re-fits the content as the Project settings choose.
                                            Some(canvas) => state.set_canvas_size(canvas, cx),
                                            None => {
                                                state.execute(Command::UpdateSettings(patch), cx);
                                            }
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
            for (label, format) in [("MP4", EncodeFormat::Mp4), ("WebM", EncodeFormat::Webm)] {
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
                let active = !self.use_bitrate && self.quality == quality;
                quality_row = quality_row.child(
                    Button::new(eid("quality", label), t(label))
                        .variant(if active { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.quality = quality;
                            this.use_bitrate = false;
                            cx.notify();
                        })),
                );
            }
            quality_row = quality_row.child(
                Button::new("quality-bitrate", t("Bitrate"))
                    .variant(if self.use_bitrate { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.use_bitrate = true;
                        cx.notify();
                    })),
            );
            let chosen = self.chosen_codec();
            let mut codec_row = div().flex().items_center().gap(px(6.)).child(
                div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Codec")),
            );
            for &codec in codecs_for(self.format) {
                let missing = self.available.as_ref().is_some_and(|codecs| !codecs.contains(&codec));
                codec_row = codec_row.child(
                    Button::new(eid("codec", codec.id()), codec.label())
                        .variant(if chosen == codec { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .disabled(missing)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.codec = Some(codec);
                            cx.notify();
                        })),
                );
            }
            let codec_note = match chosen {
                VideoCodec::H264 => t("Plays everywhere."),
                VideoCodec::Hevc => t("About a third smaller than H.264; plays on Apple devices and most new TVs."),
                VideoCodec::Av1 => t("The smallest files; slow to encode in software, and older devices cannot play it."),
                VideoCodec::Vp9 => t("Smaller than H.264; plays in every browser."),
            };
            let hardware = self.hardware;
            let encoder_row = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Encoder")))
                .child(
                    Button::new("encoder-software", t("Software"))
                        .variant(if hardware { ButtonVariant::Outline } else { ButtonVariant::Default })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hardware = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("encoder-hardware", t("Hardware if it works"))
                        .variant(if hardware { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hardware = true;
                            cx.notify();
                        })),
                );
            let bitrate_row = if self.use_bitrate {
                let input = match &self.bitrate_input {
                    Some(input) => input.clone(),
                    None => {
                        let this = cx.entity().clone();
                        let text = self.bitrate_text.clone();
                        let input = cx.new(|cx| {
                            let mut input = TextInput::new(window, cx, "8").on_commit(move |text, _, cx| {
                                let text = text.to_owned();
                                this.update(cx, |dialog, cx| {
                                    dialog.bitrate_text = text;
                                    cx.notify();
                                });
                            });
                            input.set_text(text, window, cx);
                            input
                        });
                        self.bitrate_input = Some(input.clone());
                        input
                    }
                };
                let invalid = self.video_bitrate().is_none();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Target")))
                                .child(div().w(px(80.)).child(input))
                                .child(div().text_xs().text_color(colors.muted_foreground).child(t("Mbps, then Enter"))),
                        )
                        .when(invalid, |this| {
                            this.child(div().text_xs().text_color(colors.destructive).child(tf(
                                "Enter a number from {min} to {max}",
                                &[("min", &media::MIN_BITRATE_MBPS), ("max", &media::MAX_BITRATE_MBPS)],
                            )))
                        }),
                )
            } else {
                None
            };
            let codec_missing = self.codec_missing();
            let bitrate_invalid = self.platform.is_none() && self.use_bitrate && self.video_bitrate().is_none();
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
            // Whole-mix loudness: off, or a delivery target (true peaks under -1 dBTP).
            let chosen_loudness = self.loudness;
            let mut loudness_buttons = vec![
                Button::new("loudness-off", t("Off"))
                    .variant(if chosen_loudness.is_none() { ButtonVariant::Default } else { ButtonVariant::Outline })
                    .size(ButtonSize::XSmall)
                    .disabled(!include_audio)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.loudness = None;
                        cx.notify();
                    })),
            ];
            for target in editor::LOUDNESS_TARGETS {
                let id = target.id;
                loudness_buttons.push(
                    Button::new(eid("loudness", id), format!("{:.0} LUFS", target.lufs))
                        .variant(if chosen_loudness == Some(id) { ButtonVariant::Default } else { ButtonVariant::Outline })
                        .size(ButtonSize::XSmall)
                        .disabled(!include_audio)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.loudness = Some(id);
                            cx.notify();
                        })),
                );
            }
            let loudness_note = match chosen_loudness.and_then(|id| editor::LOUDNESS_TARGETS.iter().find(|target| target.id == id)) {
                Some(target) => tf(
                    "{label}: the whole mix is measured and brought to {lufs} LUFS; a limiter holds true peaks under {peak} dBTP.",
                    &[("label", &target.label), ("lufs", &format!("{:.0}", target.lufs)), ("peak", &format!("{:.0}", target.true_peak_dbtp))],
                ),
                None => t("The mix is exported as it plays.").to_owned(),
            };
            let loudness_row = div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().w(px(80.)).text_xs().text_color(colors.muted_foreground).child(t("Loudness")))
                        .child(div().flex().flex_wrap().gap(px(4.)).children(loudness_buttons)),
                )
                .child(div().text_xs().text_color(colors.muted_foreground).child(loudness_note));
            // The quality rows go with a transparent VP9 or with a plain video.
            let (alpha_quality_row, alpha_bitrate_row, quality_row, bitrate_row) = if transparent {
                (Some(quality_row), bitrate_row, None, None)
            } else {
                (None, None, Some(quality_row), bitrate_row)
            };
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
                .child(kind_row)
                .children(kind_note.map(|note| div().text_xs().text_color(colors.muted_foreground).child(note)))
                .when(kind == Kind::Gif, |this| this.child(gif_size_row).child(gif_fps_row).child(gif_dither_row))
                .when(kind == Kind::Audio, |this| this.child(audio_format_row))
                .when(matches!(kind, Kind::Video | Kind::Still | Kind::Sequence), |this| this.child(background_row))
                .when(kind == Kind::Video && transparent, |this| this.child(alpha_row))
                .when(transparent && matches!(kind, Kind::Video | Kind::Still | Kind::Sequence), |this| {
                    this.child(div().text_xs().text_color(colors.muted_foreground).child(alpha_note))
                })
                .when(kind == Kind::Video && transparent && self.alpha_codec == AlphaCodec::Vp9, |this| {
                    this.children(alpha_quality_row).children(alpha_bitrate_row)
                })
                .when(kind == Kind::Video && !transparent, |this| this.child(platform_row).child(platform_notes))
                .when(kind == Kind::Video && !transparent && platform.is_none(), |this| {
                    this.child(format_row)
                        .child(codec_row)
                        .child(div().text_xs().text_color(colors.muted_foreground).child(codec_note))
                        .child(encoder_row)
                        .children(quality_row)
                        .children(bitrate_row)
                })
                .when(kind == Kind::Video && !transparent && codec_missing, |this| {
                    this.child(div().text_xs().text_color(colors.destructive).child(tf(
                        "This FFmpeg cannot encode {codec}.",
                        &[("codec", &chosen.label())],
                    )))
                })
                .when(kind == Kind::Video, |this| this.child(audio_row))
                .when(matches!(kind, Kind::Video | Kind::Audio), |this| this.child(loudness_row))
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
                                .disabled(
                                    self.output.is_none()
                                        || !has_ffmpeg
                                        || duration <= 0.0
                                        || (kind == Kind::Video && !transparent && (codec_missing || bitrate_invalid))
                                        || (kind == Kind::Video && transparent && self.alpha_codec == AlphaCodec::Vp9 && self.use_bitrate && self.video_bitrate().is_none()),
                                )
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

//! Rendering happens off the UI thread: one thread owns the preview
//! renderer and answers "frame at time T" requests (only the latest one
//! matters), and each export runs on a thread of its own.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

use editor::{MediaItem, MediaKind, Project, plan_audio};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::RenderImage;
use media::{AudioMixClip, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg};
use render::{Renderer, RendererOptions};
use time::MediaTime;

use crate::images::{downscale_bgra, image_from_bgra};

pub(crate) const THUMBNAIL_WIDTH: u32 = 112;

#[derive(Clone, Debug)]
pub(crate) struct ExportSettings {
    pub output: PathBuf,
    pub format: EncodeFormat,
    pub quality: EncodeQuality,
    pub include_audio: bool,
}

pub(crate) enum RenderRequest {
    Frame {
        project: Arc<Project>,
        time: MediaTime,
        width: u32,
        height: u32,
        generation: u64,
    },
    Thumbnail {
        media: MediaItem,
    },
    Forget {
        media_id: String,
    },
    Prune {
        project: Arc<Project>,
    },
    /// Stop decoder processes (playback paused for a while).
    Release,
    Export {
        project: Arc<Project>,
        settings: ExportSettings,
        cancel: Arc<AtomicBool>,
    },
}

pub(crate) enum RenderEvent {
    Frame {
        image: Arc<RenderImage>,
        time: MediaTime,
        generation: u64,
    },
    Thumbnail {
        media_id: String,
        image: Arc<RenderImage>,
    },
    /// Something the user should see (a renderer that could not start, a
    /// file that would not decode).
    Status(String),
    ExportProgress {
        frames_done: u64,
        frames_total: u64,
    },
    ExportFinished(Result<PathBuf, String>),
}

pub(crate) struct RenderHandle {
    sender: Sender<RenderRequest>,
}

impl RenderHandle {
    pub(crate) fn send(&self, request: RenderRequest) {
        self.sender.send(request).ok();
    }
}

/// Starts the render thread. Events arrive on the returned receiver, which
/// the UI awaits on its foreground executor.
pub(crate) fn spawn_render_thread(
    ffmpeg: Option<Arc<Ffmpeg>>,
) -> (RenderHandle, UnboundedReceiver<RenderEvent>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (events, event_receiver) = unbounded();
    thread::Builder::new()
        .name("opencut-render".into())
        .spawn(move || render_loop(receiver, events, ffmpeg))
        .expect("spawn render thread");
    (RenderHandle { sender }, event_receiver)
}

fn render_loop(
    receiver: Receiver<RenderRequest>,
    events: UnboundedSender<RenderEvent>,
    ffmpeg: Option<Arc<Ffmpeg>>,
) {
    let mut renderer = match Renderer::new(RendererOptions {
        ffmpeg: ffmpeg.clone(),
    }) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            events
                .unbounded_send(RenderEvent::Status(format!(
                    "The preview cannot render: {error}"
                )))
                .ok();
            None
        }
    };
    let mut queue: VecDeque<RenderRequest> = VecDeque::new();

    loop {
        if queue.is_empty() {
            match receiver.recv() {
                Ok(request) => queue.push_back(request),
                Err(_) => return,
            }
        }
        // Take everything that is waiting; only the newest frame request counts.
        loop {
            match receiver.try_recv() {
                Ok(request) => queue.push_back(request),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let latest_frame = queue
            .iter()
            .rposition(|request| matches!(request, RenderRequest::Frame { .. }));
        let mut index = 0;
        while let Some(request) = queue.pop_front() {
            let is_frame = matches!(request, RenderRequest::Frame { .. });
            let is_latest = Some(index) == latest_frame;
            index += 1;
            if is_frame && !is_latest {
                continue;
            }
            handle(request, &mut renderer, &events, &ffmpeg);
        }
    }
}

fn handle(
    request: RenderRequest,
    renderer: &mut Option<Renderer>,
    events: &UnboundedSender<RenderEvent>,
    ffmpeg: &Option<Arc<Ffmpeg>>,
) {
    match request {
        RenderRequest::Frame {
            project,
            time,
            width,
            height,
            generation,
        } => {
            let Some(renderer) = renderer.as_mut() else { return };
            match renderer.render(&project, time, width, height) {
                Ok(frame) => {
                    if let Some(image) = image_from_bgra(frame.width, frame.height, frame.bgra) {
                        events
                            .unbounded_send(RenderEvent::Frame {
                                image,
                                time,
                                generation,
                            })
                            .ok();
                    }
                }
                Err(error) => {
                    events
                        .unbounded_send(RenderEvent::Status(format!("Preview: {error}")))
                        .ok();
                }
            }
        }
        RenderRequest::Thumbnail { media } => {
            if let Some(image) = thumbnail(&media, ffmpeg) {
                events
                    .unbounded_send(RenderEvent::Thumbnail {
                        media_id: media.id,
                        image,
                    })
                    .ok();
            }
        }
        RenderRequest::Forget { media_id } => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.forget_media(&media_id);
            }
        }
        RenderRequest::Prune { project } => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.prune(&project);
            }
        }
        RenderRequest::Release => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.release_decoders();
            }
        }
        RenderRequest::Export {
            project,
            settings,
            cancel,
        } => {
            let events = events.clone();
            let ffmpeg = ffmpeg.clone();
            thread::Builder::new()
                .name("opencut-export".into())
                .spawn(move || {
                    let result = export(&project, &settings, &cancel, &events, ffmpeg);
                    events.unbounded_send(RenderEvent::ExportFinished(result)).ok();
                })
                .expect("spawn export thread");
        }
    }
}

fn thumbnail(media: &MediaItem, ffmpeg: &Option<Arc<Ffmpeg>>) -> Option<Arc<RenderImage>> {
    let frame = match media.kind {
        MediaKind::Image => media::decode_image(&media.path).ok()?,
        MediaKind::Video => {
            let ffmpeg = ffmpeg.as_ref()?;
            let duration = media.duration.map(|d| d.to_seconds_f64()).unwrap_or(0.0);
            let at = (duration * 0.1).min(1.0);
            media::thumbnail(
                ffmpeg,
                &media.path,
                at,
                THUMBNAIL_WIDTH,
                (media.width.unwrap_or(16), media.height.unwrap_or(9)),
            )
            .ok()?
        }
        MediaKind::Audio => return None,
    };
    let (width, height, bgra) = downscale_bgra(frame.width, frame.height, &frame.bgra, THUMBNAIL_WIDTH);
    image_from_bgra(width, height, bgra)
}

fn export(
    project: &Project,
    settings: &ExportSettings,
    cancel: &AtomicBool,
    events: &UnboundedSender<RenderEvent>,
    ffmpeg: Option<Arc<Ffmpeg>>,
) -> Result<PathBuf, String> {
    let ffmpeg = ffmpeg.ok_or_else(|| media::MediaError::FfmpegNotFound.to_string())?;
    let mut renderer = Renderer::new(RendererOptions {
        ffmpeg: Some(ffmpeg.clone()),
    })
    .map_err(|error| error.to_string())?;

    let fps_rate = project.settings.fps;
    let fps = fps_rate.as_f64().unwrap_or(30.0);
    let duration = project.duration();
    if duration <= MediaTime::ZERO {
        return Err("The timeline is empty".to_owned());
    }
    let frames_total = (duration.to_seconds_f64() * fps).ceil().max(1.0) as u64;
    let canvas = project.settings.canvas;
    let width = canvas.width & !1;
    let height = canvas.height & !1;

    let audio = if settings.include_audio {
        plan_audio(project)
            .into_iter()
            .map(|clip| AudioMixClip {
                path: clip.path.clone(),
                source_start_seconds: clip.source_start.to_seconds_f64(),
                source_end_seconds: clip.source_end.to_seconds_f64(),
                timeline_start_seconds: clip.timeline_start.to_seconds_f64(),
                gain: clip.gain(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let encode = EncodeSettings {
        output: settings.output.clone(),
        width,
        height,
        fps,
        format: settings.format,
        quality: settings.quality,
        duration_seconds: duration.to_seconds_f64(),
        audio,
    };
    let mut encoder = Encoder::start(&ffmpeg, &encode).map_err(|error| error.to_string())?;

    for frame_index in 0..frames_total {
        if cancel.load(Ordering::Relaxed) {
            return Err("Export cancelled".to_owned());
        }
        let time = MediaTime::from_frame(frame_index as i64, fps_rate)
            .unwrap_or(MediaTime::ZERO);
        let frame = renderer
            .render(project, time, width, height)
            .map_err(|error| error.to_string())?;
        encoder
            .write_frame(&frame.bgra)
            .map_err(|error| error.to_string())?;
        if frame_index % 5 == 0 || frame_index + 1 == frames_total {
            events
                .unbounded_send(RenderEvent::ExportProgress {
                    frames_done: frame_index + 1,
                    frames_total,
                })
                .ok();
        }
    }
    encoder.finish().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Command, Element, TrackPlacement, apply};
    use futures::StreamExt;
    use std::process::Command as Process;

    /// Renders and encodes a short project the way the export dialog does.
    /// Needs FFmpeg and a GPU adapter; skips otherwise.
    #[test]
    fn exports_a_project_with_mixed_audio() {
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        if Renderer::new(RendererOptions::default()).is_err() {
            eprintln!("no GPU adapter; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let status = Process::new(ffmpeg.ffmpeg_path())
            .args([
                "-hide_banner", "-loglevel", "error", "-y",
                "-f", "lavfi", "-i", "testsrc=size=320x180:rate=30:duration=2",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
            ])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(status.success());
        let info = media::probe(&ffmpeg, &clip).unwrap();

        let mut project = Project::new("Export test");
        project.settings.canvas = editor::CanvasSize { width: 320, height: 180 };
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "clip".to_owned(),
                name: "clip.mp4".to_owned(),
                kind: MediaKind::Video,
                path: clip,
                duration: info.duration,
                width: info.width,
                height: info.height,
                fps: info.fps,
                has_audio: true,
            }),
        )
        .unwrap();
        let element = Element::from_media(project.media_item("clip").unwrap(), MediaTime::ZERO);
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        let mut title = Element::text("Exported", MediaTime::ZERO);
        title.duration = MediaTime::from_seconds_f64(1.0).unwrap();
        apply(&mut project, &Command::InsertElement { element: title, placement: TrackPlacement::Auto }).unwrap();

        let output = dir.path().join("out.mp4");
        let (events, mut receiver) = unbounded();
        let cancel = AtomicBool::new(false);
        let result = export(
            &project,
            &ExportSettings {
                output: output.clone(),
                format: EncodeFormat::Mp4,
                quality: EncodeQuality::Low,
                include_audio: true,
            },
            &cancel,
            &events,
            Some(ffmpeg.clone()),
        );
        assert_eq!(result.unwrap(), output);
        drop(events);
        let mut last_progress = (0, 0);
        while let Some(Some(event)) = futures::executor::block_on(async { Some(receiver.next().await) }) {
            if let RenderEvent::ExportProgress { frames_done, frames_total } = event {
                last_progress = (frames_done, frames_total);
            }
        }
        assert_eq!(last_progress.1, 60, "2 s at 30 fps");
        assert_eq!(last_progress.0, 60);

        let written = media::probe(&ffmpeg, &output).unwrap();
        assert_eq!((written.width, written.height), (Some(320), Some(180)));
        assert!(written.has_audio);
        assert!((written.duration.unwrap().to_seconds_f64() - 2.0).abs() < 0.2);

        // Cancelling stops early and leaves no file behind.
        let cancelled = dir.path().join("cancelled.mp4");
        let (events, _receiver) = unbounded();
        let cancel = AtomicBool::new(true);
        let result = export(
            &project,
            &ExportSettings { output: cancelled.clone(), format: EncodeFormat::Webm, quality: EncodeQuality::Low, include_audio: false },
            &cancel,
            &events,
            Some(ffmpeg),
        );
        assert!(result.is_err());
        assert!(!cancelled.exists());
    }

    /// Exports the project `<root>/<id>/project.json` named by
    /// `OPENCUT_EXPORT_PROJECT=<root>/<id>` at its canvas size into a
    /// temporary directory. A developer aid for reproducing an export
    /// failure outside the app; skipped otherwise.
    #[test]
    fn exports_the_project_named_by_the_environment() {
        let Ok(path) = std::env::var("OPENCUT_EXPORT_PROJECT") else {
            return;
        };
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            eprintln!("ffmpeg not found; skipping");
            return;
        };
        let path = PathBuf::from(path);
        let id = path.file_name().unwrap().to_string_lossy().into_owned();
        let store = editor::ProjectStore::new(path.parent().unwrap());
        let project = store.load(&id).expect("project loads");
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.mp4");
        let (events, _receiver) = unbounded();
        let cancel = AtomicBool::new(false);
        let result = export(
            &project,
            &ExportSettings {
                output: output.clone(),
                format: EncodeFormat::Mp4,
                quality: EncodeQuality::High,
                include_audio: true,
            },
            &cancel,
            &events,
            Some(ffmpeg.clone()),
        );
        let written = result.expect("export succeeds");
        let info = media::probe(&ffmpeg, &written).unwrap();
        eprintln!("exported {} s, audio {}", info.duration.unwrap().to_seconds_f64(), info.has_audio);
    }
}

//! Rendering happens off the UI thread: one thread owns the preview
//! renderer and answers "frame at time T" requests (only the latest one
//! matters), and each export runs on a thread of its own.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

use editor::{MediaItem, MediaKind, Project};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::RenderImage;
use media::{EncodeFormat, EncodeQuality, Ffmpeg, PlatformEncode};
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
    /// Platform upload settings; replace `quality` for MP4.
    pub platform: Option<PlatformEncode>,
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
    /// Waveform peaks of a media file's audio, for the timeline.
    Peaks {
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
    /// Font files were added to the fonts directory; lay text out again.
    ReloadFonts,
    /// Decode the preview from proxies (true) or from the originals.
    SetProxies(bool),
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
    /// Peak amplitude (0..1) per [`PEAKS_PER_SECOND`]th of a second; empty
    /// when the file has no audio that could be read.
    Peaks {
        media_id: String,
        peaks: Arc<Vec<f32>>,
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
    fonts_dir: PathBuf,
    stickers_dir: PathBuf,
) -> (RenderHandle, UnboundedReceiver<RenderEvent>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (events, event_receiver) = unbounded();
    thread::Builder::new()
        .name("opencut-render".into())
        .spawn(move || render_loop(receiver, events, ffmpeg, fonts_dir, stickers_dir))
        .expect("spawn render thread");
    (RenderHandle { sender }, event_receiver)
}

fn render_loop(
    receiver: Receiver<RenderRequest>,
    events: UnboundedSender<RenderEvent>,
    ffmpeg: Option<Arc<Ffmpeg>>,
    fonts_dir: PathBuf,
    stickers_dir: PathBuf,
) {
    let mut renderer = match Renderer::new(RendererOptions {
        ffmpeg: ffmpeg.clone(),
        fonts_dir: Some(fonts_dir.clone()),
        stickers_dir: Some(stickers_dir.clone()),
        // The preview prefers proxies; `SetProxies` turns that off.
        proxy_dir: crate::settings::load().use_proxies.then(render::default_cache_dir),
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
        RenderRequest::Peaks { media } => {
            let peaks = audio_peaks(&media, ffmpeg).unwrap_or_default();
            events
                .unbounded_send(RenderEvent::Peaks {
                    media_id: media.id,
                    peaks: Arc::new(peaks),
                })
                .ok();
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
        RenderRequest::SetProxies(on) => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.set_proxy_dir(on.then(render::default_cache_dir));
            }
        }
        RenderRequest::Release => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.release_decoders();
            }
        }
        RenderRequest::ReloadFonts => {
            if let Some(renderer) = renderer.as_mut() {
                renderer.reload_fonts();
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

/// How many peaks a second of audio becomes.
pub(crate) const PEAKS_PER_SECOND: usize = 50;
const PEAKS_SAMPLE_RATE: u32 = 8_000;

/// The largest absolute sample in each 20 ms of the file's audio, mixed
/// to mono at 8 kHz through FFmpeg. None without FFmpeg or audio.
fn audio_peaks(media: &MediaItem, ffmpeg: &Option<Arc<Ffmpeg>>) -> Option<Vec<f32>> {
    let ffmpeg = ffmpeg.as_ref()?;
    if !media.has_audio {
        return None;
    }
    let settings = media::AudioSettings { sample_rate: PEAKS_SAMPLE_RATE, channels: 1 };
    let mut reader = media::AudioReader::open(ffmpeg, &media.path, 0.0, settings).ok()?;
    let bucket = PEAKS_SAMPLE_RATE as usize / PEAKS_PER_SECOND;
    let mut buffer = vec![0.0f32; bucket * 64];
    let mut carry: Vec<f32> = Vec::new();
    let mut peaks = Vec::new();
    loop {
        let got = reader.read(&mut buffer).ok()?;
        if got == 0 {
            break;
        }
        carry.extend_from_slice(&buffer[..got]);
        let full = carry.len() / bucket;
        for index in 0..full {
            let slice = &carry[index * bucket..(index + 1) * bucket];
            peaks.push(slice.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs())).min(1.0));
        }
        carry.drain(..full * bucket);
    }
    if !carry.is_empty() {
        peaks.push(carry.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs())).min(1.0));
    }
    Some(peaks)
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
        fonts_dir: Some(crate::fonts::fonts_dir(&editor::ProjectStore::default_root())),
        stickers_dir: Some(crate::stickers::stickers_dir(&editor::ProjectStore::default_root())),
        // Export always reads the original media.
        proxy_dir: None,
    })
    .map_err(|error| error.to_string())?;

    let job = render::ExportJob {
        output: settings.output.clone(),
        format: settings.format,
        quality: settings.quality,
        platform: settings.platform,
        include_audio: settings.include_audio,
        cache_dir: None,
    };
    render::export_project(
        &mut renderer,
        &ffmpeg,
        project,
        &job,
        &|| cancel.load(Ordering::Relaxed),
        &mut |frames_done, frames_total| {
            events.unbounded_send(RenderEvent::ExportProgress { frames_done, frames_total }).ok();
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Command, Element, TrackPlacement, apply};
    use futures::StreamExt;
    use std::process::Command as Process;

    /// Peaks of a synthetic clip: a 440 Hz tone for one second, then a
    /// second of silence. Needs FFmpeg; skips otherwise.
    #[test]
    fn peaks_follow_the_audio() {
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let status = Process::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "aevalsrc=0.8*sin(440*2*PI*t)*lt(t\\,1):d=2"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let media = MediaItem {
            id: "tone".to_owned(),
            name: "tone.wav".to_owned(),
            kind: MediaKind::Audio,
            path,
            duration: MediaTime::from_seconds_f64(2.0),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let peaks = audio_peaks(&media, &Some(ffmpeg)).unwrap();
        assert!((peaks.len() as i64 - 2 * PEAKS_PER_SECOND as i64).abs() <= 2, "{}", peaks.len());
        let loud = &peaks[5..PEAKS_PER_SECOND - 5];
        let quiet = &peaks[PEAKS_PER_SECOND + 5..];
        assert!(loud.iter().all(|peak| *peak > 0.6), "tone second is loud: {loud:?}");
        assert!(quiet.iter().all(|peak| *peak < 0.01), "silent second is quiet");
    }

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
        let clip_id = element.id.clone();
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        // A keyframed fade-out, so the export runs ffmpeg's volume expression.
        for (seconds, value) in [(0.0, 0.0), (2.0, -40.0)] {
            apply(
                &mut project,
                &Command::SetKeyframe {
                    element_id: clip_id.clone(),
                    path: "volume".to_owned(),
                    time: MediaTime::from_seconds_f64(seconds).unwrap(),
                    value,
                    segment: None,
                },
            )
            .unwrap();
        }
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
                platform: None,
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
            &ExportSettings { output: cancelled.clone(), format: EncodeFormat::Webm, quality: EncodeQuality::Low, include_audio: false, platform: None },
            &cancel,
            &events,
            Some(ffmpeg),
        );
        assert!(result.is_err());
        assert!(!cancelled.exists());
    }

    /// Exports the project `<root>/<id>/project.json` named by
    /// `OPENCUT_EXPORT_PROJECT=<root>/<id>` at its canvas size into a
    /// temporary directory, with `OPENCUT_EXPORT_PRESET=<platform id>` for a
    /// platform preset. A developer aid for reproducing an export failure
    /// outside the app; skipped otherwise.
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
        // The file is kept where OPENCUT_EXPORT_OUTPUT points, otherwise discarded.
        let dir = tempfile::tempdir().unwrap();
        let output = std::env::var_os("OPENCUT_EXPORT_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| dir.path().join("out.mp4"));
        let platform = std::env::var("OPENCUT_EXPORT_PRESET").ok().map(|id| {
            let preset = editor::platform_preset(&id).expect("a known platform preset");
            let fps = project.settings.fps.as_f64().unwrap_or(30.0);
            let encode = editor::platform_encode_settings(project.settings.canvas, fps);
            eprintln!("preset {}: {encode:?}", preset.label);
            PlatformEncode {
                video_bitrate: encode.video_bitrate,
                audio_bitrate: encode.audio_bitrate,
                gop_frames: encode.gop_frames,
            }
        });
        let (events, _receiver) = unbounded();
        let cancel = AtomicBool::new(false);
        let result = export(
            &project,
            &ExportSettings {
                output: output.clone(),
                format: EncodeFormat::Mp4,
                quality: EncodeQuality::High,
                include_audio: true,
                platform,
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

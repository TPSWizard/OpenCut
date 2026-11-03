//! The open project and everything that moves around it: selection,
//! playback, the preview image, saving, thumbnails and export. Panels read
//! this entity and call its methods; it notifies them when anything changes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use editor::{
    Command, Edge, Editor, Element, ElementPatch, MediaItem, MediaKind, Outcome, Params, Project,
    ProjectStore, TrackPlacement, nearest_free_start, plan_audio,
};
use futures::StreamExt;
use gpui::{Context, FocusHandle, RenderImage, Task};
use media::{Ffmpeg, is_image_path, probe};
use time::MediaTime;

use crate::audio::AudioOutput;
use crate::render_thread::{
    ExportSettings, RenderEvent, RenderHandle, RenderRequest, spawn_render_thread,
};

pub(crate) const MIN_ZOOM: f32 = 4.0;
pub(crate) const MAX_ZOOM: f32 = 1200.0;
const SAVE_DELAY: Duration = Duration::from_millis(600);
const RELEASE_DECODERS_AFTER: Duration = Duration::from_secs(20);

pub(crate) struct ExportState {
    pub frames_done: u64,
    pub frames_total: u64,
    pub finished: Option<Result<PathBuf, String>>,
    cancel: Arc<AtomicBool>,
}

impl ExportState {
    pub(crate) fn progress(&self) -> f32 {
        if self.frames_total == 0 {
            0.0
        } else {
            self.frames_done as f32 / self.frames_total as f32
        }
    }
}

pub(crate) struct EditorState {
    /// Focus for the editor's own shortcuts; panels focus it on click so a
    /// text field stops receiving keys.
    pub focus_handle: FocusHandle,
    pub editor: Editor,
    store: ProjectStore,
    ffmpeg: Option<Arc<Ffmpeg>>,
    pub selection: Vec<String>,
    pub playing: bool,
    play_anchor: Option<(Instant, MediaTime)>,
    /// Timeline pixels per second.
    pub zoom: f32,
    /// Timeline horizontal scroll, in seconds.
    pub scroll_seconds: f64,
    pub preview: Option<Arc<RenderImage>>,
    pub preview_time: Option<MediaTime>,
    preview_size: (u32, u32),
    pub status: Option<String>,
    pub thumbnails: HashMap<String, Arc<RenderImage>>,
    pub export: Option<ExportState>,
    render: RenderHandle,
    audio: Option<AudioOutput>,
    render_generation: u64,
    dirty: bool,
    save_task: Option<Task<()>>,
    release_task: Option<Task<()>>,
    playback_task: Option<Task<()>>,
    _event_task: Task<()>,
}

impl EditorState {
    pub(crate) fn new(
        project: Project,
        store: ProjectStore,
        ffmpeg: Option<Arc<Ffmpeg>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (render, mut events) = spawn_render_thread(ffmpeg.clone());
        let event_task = cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update(cx, |state, cx| state.handle_render_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut status = None;
        let audio = match ffmpeg.clone() {
            Some(ffmpeg) => match AudioOutput::open(ffmpeg) {
                Ok(audio) => Some(audio),
                Err(error) => {
                    status = Some(format!("No audio playback: {error}"));
                    None
                }
            },
            None => {
                status = Some(
                    "FFmpeg was not found: video and audio files cannot be imported or exported. Install it and put ffmpeg and ffprobe on PATH (or set OPENCUT_FFMPEG_DIR)."
                        .to_owned(),
                );
                None
            }
        };
        let mut state = Self {
            focus_handle: cx.focus_handle(),
            editor: Editor::new(project),
            store,
            ffmpeg,
            selection: Vec::new(),
            playing: false,
            play_anchor: None,
            zoom: 60.0,
            scroll_seconds: 0.0,
            preview: None,
            preview_time: None,
            preview_size: (640, 360),
            status,
            thumbnails: HashMap::new(),
            export: None,
            render,
            audio,
            render_generation: 0,
            dirty: false,
            save_task: None,
            release_task: None,
            playback_task: None,
            _event_task: event_task,
        };
        for media in state.editor.project().media.clone() {
            state.render.send(RenderRequest::Thumbnail { media });
        }
        state.sync_audio_clips();
        state.request_render(cx);
        state
    }

    pub(crate) fn project(&self) -> &Project {
        self.editor.project()
    }

    pub(crate) fn playhead(&self) -> MediaTime {
        self.project().playhead
    }

    pub(crate) fn duration(&self) -> MediaTime {
        self.project().duration()
    }

    pub(crate) fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some()
    }

    // ---- editing -----------------------------------------------------

    /// Runs an undoable command. Errors become a status message.
    pub(crate) fn execute(&mut self, command: Command, cx: &mut Context<Self>) -> Option<Outcome> {
        match self.editor.execute(&command) {
            Ok(outcome) => {
                self.after_change(cx);
                Some(outcome)
            }
            Err(error) => {
                self.set_status(error.to_string(), cx);
                None
            }
        }
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.editor.undo().is_ok() {
            self.after_change(cx);
        }
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.editor.redo().is_ok() {
            self.after_change(cx);
        }
    }

    fn after_change(&mut self, cx: &mut Context<Self>) {
        let present: Vec<String> = self
            .selection
            .iter()
            .filter(|id| self.project().locate(id).is_some())
            .cloned()
            .collect();
        self.selection = present;
        let duration = self.project().duration();
        if self.project().playhead > duration {
            self.editor
                .apply_transient(|project| project.playhead = duration);
        }
        self.dirty = true;
        self.schedule_save(cx);
        self.sync_audio_clips();
        self.render
            .send(RenderRequest::Prune { project: Arc::new(self.project().clone()) });
        self.request_render(cx);
        cx.notify();
    }

    pub(crate) fn set_status(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.status = Some(message.into());
        cx.notify();
    }

    pub(crate) fn clear_status(&mut self, cx: &mut Context<Self>) {
        self.status = None;
        cx.notify();
    }

    // ---- selection ---------------------------------------------------

    pub(crate) fn is_selected(&self, element_id: &str) -> bool {
        self.selection.iter().any(|id| id == element_id)
    }

    pub(crate) fn select(&mut self, element_id: &str, extend: bool, cx: &mut Context<Self>) {
        if extend {
            if let Some(index) = self.selection.iter().position(|id| id == element_id) {
                self.selection.remove(index);
            } else {
                self.selection.push(element_id.to_owned());
            }
        } else if !self.is_selected(element_id) {
            self.selection = vec![element_id.to_owned()];
        }
        cx.notify();
    }

    pub(crate) fn select_only(&mut self, element_ids: Vec<String>, cx: &mut Context<Self>) {
        self.selection = element_ids;
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            self.selection.clear();
            cx.notify();
        }
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection = self
            .project()
            .elements()
            .map(|(_, element)| element.id.clone())
            .collect();
        cx.notify();
    }

    /// The single selected element, if exactly one is selected.
    pub(crate) fn selected_element(&self) -> Option<&Element> {
        match self.selection.as_slice() {
            [id] => self.project().element(id).map(|(_, element)| element),
            _ => None,
        }
    }

    pub(crate) fn delete_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            return;
        }
        let ids = std::mem::take(&mut self.selection);
        self.execute(Command::DeleteElements(ids), cx);
    }

    pub(crate) fn duplicate_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            return;
        }
        let ids = self.selection.clone();
        if let Some(outcome) = self.execute(Command::DuplicateElements(ids), cx) {
            let created: Vec<String> = outcome
                .element_refs()
                .iter()
                .map(|r| r.element_id.clone())
                .collect();
            self.select_only(created, cx);
        }
    }

    /// Splits the selected elements at the playhead, or every element
    /// under the playhead when nothing is selected.
    pub(crate) fn split_at_playhead(&mut self, cx: &mut Context<Self>) {
        let time = self.playhead();
        let targets: Vec<String> = if self.selection.is_empty() {
            self.project()
                .elements()
                .filter(|(_, element)| element.contains(time))
                .map(|(_, element)| element.id.clone())
                .collect()
        } else {
            self.selection
                .iter()
                .filter(|id| {
                    self.project()
                        .element(id)
                        .map(|(_, element)| element.contains(time))
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        };
        if targets.is_empty() {
            self.set_status("Nothing under the playhead to split", cx);
            return;
        }
        let mut new_selection = Vec::new();
        for element_id in targets {
            if let Some(outcome) = self.execute(Command::SplitElement { element_id, time }, cx) {
                new_selection.extend(
                    outcome
                        .element_refs()
                        .iter()
                        .map(|r| r.element_id.clone()),
                );
            }
        }
        self.select_only(new_selection, cx);
    }

    pub(crate) fn update_selected_params(&mut self, params: Params, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::UpdateElement {
                element_id,
                patch: ElementPatch {
                    params: Some(params),
                    ..Default::default()
                },
            },
            cx,
        );
    }

    pub(crate) fn patch_selected(&mut self, patch: ElementPatch, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(Command::UpdateElement { element_id, patch }, cx);
    }

    // ---- media -------------------------------------------------------

    /// Probes files on a background thread and adds them to the project.
    pub(crate) fn import_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let ffmpeg = self.ffmpeg.clone();
        self.set_status(format!("Importing {} file(s)…", paths.len()), cx);
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| probe_media(ffmpeg.as_deref(), path))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |state, cx| {
                let mut failures = Vec::new();
                let mut added = 0;
                for result in results {
                    match result {
                        Ok(media) => {
                            let request = RenderRequest::Thumbnail {
                                media: media.clone(),
                            };
                            if state.execute(Command::AddMedia(media), cx).is_some() {
                                added += 1;
                                state.render.send(request);
                            }
                        }
                        Err(message) => failures.push(message),
                    }
                }
                if failures.is_empty() {
                    state.set_status(format!("Imported {added} file(s)"), cx);
                } else {
                    state.set_status(failures.join("; "), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn remove_media(&mut self, media_id: &str, cx: &mut Context<Self>) {
        self.thumbnails.remove(media_id);
        self.render.send(RenderRequest::Forget {
            media_id: media_id.to_owned(),
        });
        self.execute(
            Command::RemoveMedia {
                media_id: media_id.to_owned(),
            },
            cx,
        );
    }

    /// Puts a media item on the timeline at the playhead (or right after
    /// whatever is there) and selects it.
    pub(crate) fn add_media_to_timeline(&mut self, media_id: &str, cx: &mut Context<Self>) {
        let Some(media) = self.project().media_item(media_id).cloned() else {
            return;
        };
        let element = Element::from_media(&media, self.playhead());
        self.insert_element(element, cx);
    }

    pub(crate) fn add_text(&mut self, cx: &mut Context<Self>) {
        let element = Element::text("Your text", self.playhead());
        self.insert_element(element, cx);
    }

    fn insert_element(&mut self, element: Element, cx: &mut Context<Self>) {
        if let Some(outcome) = self.execute(
            Command::InsertElement {
                element,
                placement: TrackPlacement::Auto,
            },
            cx,
        ) {
            let ids: Vec<String> = outcome
                .element_refs()
                .iter()
                .map(|r| r.element_id.clone())
                .collect();
            self.select_only(ids, cx);
        }
    }

    /// Moves an element to a track and the closest free time to `desired`.
    pub(crate) fn move_element(
        &mut self,
        element_id: &str,
        track_id: Option<&str>,
        desired: MediaTime,
        cx: &mut Context<Self>,
    ) {
        let project = self.project();
        let Some((from_track, element)) = project.element(element_id) else {
            return;
        };
        let target_id = track_id.unwrap_or(&from_track.id).to_owned();
        let Some(target) = project.track(&target_id) else {
            return;
        };
        if !target.accepts(&element.kind) {
            return;
        }
        let ignore = (target.id == from_track.id).then_some(element_id);
        let Some(start) = nearest_free_start(target, element.duration, desired, ignore) else {
            return;
        };
        if start == element.start_time && target.id == from_track.id {
            return;
        }
        self.execute(
            Command::MoveElement {
                element_id: element_id.to_owned(),
                track_id: Some(target_id),
                start_time: start,
            },
            cx,
        );
    }

    pub(crate) fn resize_element(
        &mut self,
        element_id: &str,
        edge: Edge,
        time: MediaTime,
        cx: &mut Context<Self>,
    ) {
        self.execute(
            Command::ResizeElement {
                element_id: element_id.to_owned(),
                edge,
                time,
            },
            cx,
        );
    }

    // ---- playback ----------------------------------------------------

    pub(crate) fn seek(&mut self, time: MediaTime, cx: &mut Context<Self>) {
        let time = time.clamp(MediaTime::ZERO, self.duration().max(MediaTime::ZERO));
        self.editor.apply_transient(|project| project.playhead = time);
        if self.playing {
            self.play_anchor = Some((Instant::now(), time));
            if let Some(audio) = &self.audio {
                audio.seek(time.to_seconds_f64());
            }
        }
        self.request_render(cx);
        cx.notify();
    }

    pub(crate) fn step_frames(&mut self, frames: i64, cx: &mut Context<Self>) {
        let rate = self.project().settings.fps;
        let ticks = rate.ticks_per_frame().unwrap_or(4000);
        let current = self.playhead().floor_to_frame(rate).unwrap_or(self.playhead());
        self.seek(current + MediaTime::from_ticks(ticks * frames), cx);
    }

    pub(crate) fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if self.playing {
            self.pause(cx);
        } else {
            self.play(cx);
        }
    }

    pub(crate) fn play(&mut self, cx: &mut Context<Self>) {
        if self.playing || self.duration() <= MediaTime::ZERO {
            return;
        }
        if self.playhead() >= self.duration() {
            self.editor
                .apply_transient(|project| project.playhead = MediaTime::ZERO);
        }
        self.playing = true;
        self.release_task = None;
        let start = self.playhead();
        self.play_anchor = Some((Instant::now(), start));
        if let Some(audio) = &self.audio {
            audio.seek(start.to_seconds_f64());
            audio.play();
        }
        let frame_interval = Duration::from_millis(
            (1000.0 / self.project().settings.fps.as_f64().unwrap_or(30.0)).round() as u64,
        )
        .max(Duration::from_millis(8));
        self.playback_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(frame_interval).await;
                let keep_going = this
                    .update(cx, |state, cx| state.tick(cx))
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub(crate) fn pause(&mut self, cx: &mut Context<Self>) {
        if !self.playing {
            return;
        }
        self.playing = false;
        self.play_anchor = None;
        self.playback_task = None;
        if let Some(audio) = &self.audio {
            audio.pause();
        }
        // Snap to a frame so edits land on frame boundaries.
        let rate = self.project().settings.fps;
        let snapped = self
            .playhead()
            .round_to_frame(rate)
            .unwrap_or(self.playhead());
        self.editor
            .apply_transient(|project| project.playhead = snapped);
        self.request_render(cx);
        self.schedule_release(cx);
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.playing {
            return false;
        }
        let Some((anchor_instant, anchor_time)) = self.play_anchor else {
            return false;
        };
        let elapsed = MediaTime::from_seconds_f64(anchor_instant.elapsed().as_secs_f64())
            .unwrap_or(MediaTime::ZERO);
        let time = anchor_time + elapsed;
        let duration = self.duration();
        if time >= duration {
            self.editor
                .apply_transient(|project| project.playhead = duration);
            self.pause(cx);
            return false;
        }
        self.editor.apply_transient(|project| project.playhead = time);
        self.request_render(cx);
        cx.notify();
        true
    }

    fn sync_audio_clips(&self) {
        if let Some(audio) = &self.audio {
            audio.set_clips(plan_audio(self.project()));
        }
    }

    // ---- preview -----------------------------------------------------

    /// Called by the preview panel with the pixels it has to show the frame in.
    pub(crate) fn set_preview_size(&mut self, width: u32, height: u32, cx: &mut Context<Self>) {
        let canvas = self.project().settings.canvas;
        // Render at the shown size, never larger than the canvas.
        let (width, height) = fit_size(width, height, canvas.width, canvas.height);
        if (width, height) != self.preview_size && width >= 2 && height >= 2 {
            self.preview_size = (width, height);
            self.request_render(cx);
        }
    }

    pub(crate) fn request_render(&mut self, _cx: &mut Context<Self>) {
        self.render_generation += 1;
        let (width, height) = self.preview_size;
        self.render.send(RenderRequest::Frame {
            project: Arc::new(self.project().clone()),
            time: self.playhead(),
            width,
            height,
            generation: self.render_generation,
        });
    }

    fn schedule_release(&mut self, cx: &mut Context<Self>) {
        self.release_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RELEASE_DECODERS_AFTER).await;
            this.update(cx, |state, _| {
                if !state.playing {
                    state.render.send(RenderRequest::Release);
                }
                state.release_task = None;
            })
            .ok();
        }));
    }

    fn handle_render_event(&mut self, event: RenderEvent, cx: &mut Context<Self>) {
        match event {
            RenderEvent::Frame {
                image,
                time,
                generation,
            } => {
                // Older frames can arrive after newer ones were requested;
                // they are still the newest we have.
                if generation + 2 >= self.render_generation || self.preview.is_none() {
                    self.preview = Some(image);
                    self.preview_time = Some(time);
                    cx.notify();
                }
            }
            RenderEvent::Thumbnail { media_id, image } => {
                self.thumbnails.insert(media_id, image);
                cx.notify();
            }
            RenderEvent::Status(message) => self.set_status(message, cx),
            RenderEvent::ExportProgress {
                frames_done,
                frames_total,
            } => {
                if let Some(export) = &mut self.export {
                    export.frames_done = frames_done;
                    export.frames_total = frames_total;
                    cx.notify();
                }
            }
            RenderEvent::ExportFinished(result) => {
                if let Some(export) = &mut self.export {
                    export.finished = Some(result);
                    cx.notify();
                }
            }
        }
    }

    // ---- saving ------------------------------------------------------

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.save_task.is_some() {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            this.update(cx, |state, cx| {
                state.save_task = None;
                state.save_now(cx);
            })
            .ok();
        }));
    }

    pub(crate) fn save_now(&mut self, cx: &mut Context<Self>) {
        match self.store.save(self.project()) {
            Ok(()) => {
                self.dirty = false;
            }
            Err(error) => self.set_status(format!("Could not save: {error}"), cx),
        }
        cx.notify();
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn rename_project(&mut self, name: String, cx: &mut Context<Self>) {
        self.execute(Command::RenameProject(name), cx);
    }

    // ---- export ------------------------------------------------------

    pub(crate) fn start_export(&mut self, settings: ExportSettings, cx: &mut Context<Self>) {
        if self.export.as_ref().is_some_and(|e| e.finished.is_none()) {
            return;
        }
        self.pause(cx);
        let cancel = Arc::new(AtomicBool::new(false));
        self.export = Some(ExportState {
            frames_done: 0,
            frames_total: 0,
            finished: None,
            cancel: Arc::clone(&cancel),
        });
        self.render.send(RenderRequest::Export {
            project: Arc::new(self.project().clone()),
            settings,
            cancel,
        });
        cx.notify();
    }

    pub(crate) fn cancel_export(&mut self, cx: &mut Context<Self>) {
        if let Some(export) = &self.export {
            export.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    pub(crate) fn dismiss_export(&mut self, cx: &mut Context<Self>) {
        if self.export.as_ref().is_some_and(|e| e.finished.is_some()) {
            self.export = None;
            cx.notify();
        }
    }

    // ---- timeline view -----------------------------------------------

    pub(crate) fn zoom_by(&mut self, factor: f32, anchor_seconds: Option<f64>, cx: &mut Context<Self>) {
        let old = self.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if let Some(anchor) = anchor_seconds {
            // Keep the time under the pointer where it is.
            let pixels = (anchor - self.scroll_seconds) * old as f64;
            self.scroll_seconds = (anchor - pixels / new as f64).max(0.0);
        }
        self.zoom = new;
        cx.notify();
    }

    pub(crate) fn scroll_by_pixels(&mut self, pixels: f32, cx: &mut Context<Self>) {
        self.scroll_seconds = (self.scroll_seconds + pixels as f64 / self.zoom as f64).max(0.0);
        cx.notify();
    }

    /// Scrolls so the playhead stays visible while playing.
    pub(crate) fn follow_playhead(&mut self, visible_width: f32, cx: &mut Context<Self>) {
        let playhead = self.playhead().to_seconds_f64();
        let visible_seconds = visible_width as f64 / self.zoom as f64;
        if playhead < self.scroll_seconds || playhead > self.scroll_seconds + visible_seconds {
            self.scroll_seconds = (playhead - visible_seconds * 0.1).max(0.0);
            cx.notify();
        }
    }

    /// Saves and stops playback and decoders before the editor closes.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.pause(cx);
        self.cancel_export(cx);
        if self.dirty {
            self.save_now(cx);
        }
        self.render.send(RenderRequest::Release);
    }
}

/// Fits a `canvas`-sized frame into `width` x `height`, rounded to even.
fn fit_size(width: u32, height: u32, canvas_width: u32, canvas_height: u32) -> (u32, u32) {
    let (fit_w, fit_h) = editor::fit_contain(
        canvas_width as f32,
        canvas_height as f32,
        width as f32,
        height as f32,
    );
    let w = (fit_w.round() as u32).min(canvas_width).max(2) & !1;
    let h = (fit_h.round() as u32).min(canvas_height).max(2) & !1;
    (w, h)
}

fn probe_media(ffmpeg: Option<&Ffmpeg>, path: PathBuf) -> Result<MediaItem, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if is_image_path(&path) {
        let (width, height) =
            image::image_dimensions(&path).map_err(|error| format!("{name}: {error}"))?;
        return Ok(MediaItem {
            id: editor::new_id(),
            name,
            kind: MediaKind::Image,
            path,
            duration: None,
            width: Some(width),
            height: Some(height),
            fps: None,
            has_audio: false,
        });
    }
    let Some(ffmpeg) = ffmpeg else {
        return Err(format!("{name}: FFmpeg is needed to import video and audio"));
    };
    let info = probe(ffmpeg, &path).map_err(|error| format!("{name}: {error}"))?;
    let kind = match info.kind {
        media::MediaKind::Video => MediaKind::Video,
        media::MediaKind::Image => MediaKind::Image,
        media::MediaKind::Audio => MediaKind::Audio,
    };
    if kind == MediaKind::Video && info.duration.is_none() {
        return Err(format!("{name}: could not read the duration"));
    }
    Ok(MediaItem {
        id: editor::new_id(),
        name,
        kind,
        path,
        duration: info.duration,
        width: info.width,
        height: info.height,
        fps: info.fps,
        has_audio: info.has_audio,
    })
}


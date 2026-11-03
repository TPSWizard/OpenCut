//! The open project and everything that moves around it: selection,
//! playback, the preview image, saving, thumbnails and export. Panels read
//! this entity and call its methods; it notifies them when anything changes.

/// `OPENCUT_TRACE_INPUT=1` prints scroll-wheel events and scroll offsets
/// to stderr, for driving the app from a script.
pub(crate) fn trace_input() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("OPENCUT_TRACE_INPUT").is_some_and(|v| !v.is_empty() && v != "0"))
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use editor::{
    Command, Edge, Editor, Effect, Element, ElementPatch, Mask, MediaItem, MediaKind, Outcome,
    Params, Project, ProjectStore, TrackPlacement, nearest_free_start, plan_audio,
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

/// A freeform mask being drawn with the pen.
pub(crate) struct PenTool {
    pub element_id: String,
    pub mask_id: String,
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
    /// The freeform mask being drawn on the preview, if any.
    pub pen: Option<PenTool>,
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
            pen: None,
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
        if self.pen.is_some() && (extend || !self.is_selected(element_id)) {
            self.finish_pen(cx);
        }
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
        if self.pen.is_some() {
            self.finish_pen(cx);
        }
        self.selection = element_ids;
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        // Escape while drawing cancels the drawing, not the selection.
        if self.pen.is_some() {
            self.cancel_pen(cx);
            return;
        }
        if !self.selection.is_empty() {
            self.selection.clear();
            cx.notify();
        }
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        if self.pen.is_some() {
            self.finish_pen(cx);
        }
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

    /// Changes parameters of the selected element. A parameter that is
    /// keyframed gets a key at the playhead instead, as in the web app.
    pub(crate) fn update_selected_params(&mut self, params: Params, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let time = self.playhead();
        let mut plain = Params::new();
        let mut keyed = Vec::new();
        for (key, value) in params.iter() {
            match value.as_number() {
                Some(number) if element.is_animated(key) => keyed.push((key.clone(), number)),
                _ => {
                    plain.set(key.clone(), value.clone());
                }
            }
        }
        for (path, value) in keyed {
            self.execute(
                Command::SetKeyframe {
                    element_id: element_id.clone(),
                    path,
                    time,
                    value,
                    segment: None,
                },
                cx,
            );
        }
        if !plain.is_empty() {
            self.execute(
                Command::UpdateElement {
                    element_id,
                    patch: ElementPatch {
                        params: Some(plain),
                        ..Default::default()
                    },
                },
                cx,
            );
        }
    }

    // ---- keyframes, effects and masks ------------------------------------

    /// Adds a key of `path` at the playhead holding the current value, or
    /// removes the key already there.
    pub(crate) fn toggle_keyframe(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let time = self.playhead();
        let local = element.local_time(time);
        if let Some(key) = element
            .animations
            .get(path)
            .and_then(|channel| channel.key_at(local))
        {
            let keyframe_id = key.id.clone();
            self.execute(
                Command::RemoveKeyframe {
                    element_id,
                    path: path.to_owned(),
                    keyframe_id,
                },
                cx,
            );
            return;
        }
        let Some(value) = animated_value(element, path, time) else {
            self.set_status(format!("{path} has no value to key"), cx);
            return;
        };
        self.execute(
            Command::SetKeyframe {
                element_id,
                path: path.to_owned(),
                time,
                value,
                segment: None,
            },
            cx,
        );
    }

    pub(crate) fn clear_animation(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::ClearAnimation {
                element_id,
                path: path.to_owned(),
            },
            cx,
        );
    }

    /// Moves the playhead to the selected element's next (or previous)
    /// keyframe, on any of its parameters.
    pub(crate) fn jump_to_keyframe(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else {
            self.set_status("Select a clip to step through its keyframes", cx);
            return;
        };
        let playhead = self.playhead();
        let mut times: Vec<MediaTime> = element
            .animations
            .values()
            .flat_map(|channel| channel.all_keys().map(|key| element.start_time + key.time))
            .collect();
        times.sort();
        times.dedup();
        let target = if forward {
            times.into_iter().find(|&time| time > playhead)
        } else {
            times.into_iter().rev().find(|&time| time < playhead)
        };
        match target {
            Some(time) => self.seek(time, cx),
            None => self.set_status("No keyframe in that direction", cx),
        }
    }

    pub(crate) fn add_effect(&mut self, effect: Effect, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(Command::AddEffect { element_id, effect }, cx);
    }

    pub(crate) fn update_effect(
        &mut self,
        effect_id: &str,
        params: Option<Params>,
        enabled: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        // Keyframed effect parameters get a key at the playhead.
        let time = self.playhead();
        let mut plain = Params::new();
        let mut keyed = Vec::new();
        if let Some(params) = params {
            for (key, value) in params.iter() {
                let path = format!("effects.{effect_id}.params.{key}");
                match value.as_number() {
                    Some(number) if element.is_animated(&path) => keyed.push((path, number)),
                    _ => {
                        plain.set(key.clone(), value.clone());
                    }
                }
            }
        }
        for (path, value) in keyed {
            self.execute(
                Command::SetKeyframe {
                    element_id: element_id.clone(),
                    path,
                    time,
                    value,
                    segment: None,
                },
                cx,
            );
        }
        if !plain.is_empty() || enabled.is_some() {
            self.execute(
                Command::UpdateEffect {
                    element_id,
                    effect_id: effect_id.to_owned(),
                    params: (!plain.is_empty()).then_some(plain),
                    enabled,
                },
                cx,
            );
        }
    }

    pub(crate) fn remove_effect(&mut self, effect_id: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::RemoveEffect {
                element_id,
                effect_id: effect_id.to_owned(),
            },
            cx,
        );
    }

    /// Starts drawing a freeform mask on the selected clip: it replaces any
    /// mask the clip has, and points are added by clicking on the preview.
    pub(crate) fn start_pen(&mut self, cx: &mut Context<Self>) {
        if self.pen.is_some() {
            self.finish_pen(cx);
        }
        let Some(element) = self.selected_element() else { return };
        if !element.can_be_masked() {
            return;
        }
        let element_id = element.id.clone();
        let existing: Vec<String> = element.masks.iter().map(|mask| mask.id.clone()).collect();
        for mask_id in existing {
            self.execute(Command::RemoveMask { element_id: element_id.clone(), mask_id }, cx);
        }
        let mask = Mask::shape(Mask::FREEFORM);
        let mask_id = mask.id.clone();
        self.execute(Command::AddMask { element_id: element_id.clone(), mask }, cx);
        self.pen = Some(PenTool { element_id, mask_id });
        self.set_status(
            "Pen: click on the picture to add points; double-click or Enter closes the path, Escape cancels",
            cx,
        );
    }

    /// The pen's mask, as long as its element is still the selection.
    fn pen_mask(&self) -> Option<(&Element, &Mask)> {
        let pen = self.pen.as_ref()?;
        let element = self.selected_element().filter(|element| element.id == pen.element_id)?;
        let mask = element.mask(&pen.mask_id)?;
        Some((element, mask))
    }

    /// Adds a corner point at a canvas pixel.
    pub(crate) fn pen_add_point(&mut self, canvas_x: f64, canvas_y: f64, cx: &mut Context<Self>) {
        let playhead = self.playhead();
        let Some((element, mask)) = self.pen_mask() else {
            self.pen = None;
            return;
        };
        let Some(quad) = editor::element_quad(self.project(), element, playhead) else { return };
        let (x, y) = editor::canvas_to_freeform_local(&quad, &mask.params, canvas_x, canvas_y);
        let mut points = editor::read_freeform_path(&mask.params);
        points.push(editor::FreeformPoint::corner(x, y));
        let count = points.len();
        let (element_id, mask_id) = (element.id.clone(), mask.id.clone());
        let mut params = Params::new();
        params.set("path", editor::write_freeform_path(&points));
        self.execute(Command::UpdateMask { element_id, mask_id, params }, cx);
        self.set_status(
            format!("Pen: {count} point(s); double-click or Enter closes the path, Escape cancels"),
            cx,
        );
    }

    /// Pulls the last point's handles out along a drag from where it was
    /// placed (`from`) to where the button came up (`to`), both in canvas
    /// pixels: the out handle follows the drag, the in handle mirrors it,
    /// so the path bends smoothly through the point.
    pub(crate) fn pen_drag_handles(&mut self, from: (f64, f64), to: (f64, f64), cx: &mut Context<Self>) {
        let playhead = self.playhead();
        let Some((element, mask)) = self.pen_mask() else { return };
        let Some(quad) = editor::element_quad(self.project(), element, playhead) else { return };
        let (fx, fy) = editor::canvas_to_freeform_local(&quad, &mask.params, from.0, from.1);
        let (tx, ty) = editor::canvas_to_freeform_local(&quad, &mask.params, to.0, to.1);
        let mut points = editor::read_freeform_path(&mask.params);
        let Some(last) = points.last_mut() else { return };
        last.out_x = tx - fx;
        last.out_y = ty - fy;
        last.in_x = fx - tx;
        last.in_y = fy - ty;
        let (element_id, mask_id) = (element.id.clone(), mask.id.clone());
        let mut params = Params::new();
        params.set("path", editor::write_freeform_path(&points));
        self.execute(Command::UpdateMask { element_id, mask_id, params }, cx);
    }

    /// Closes the path (three points or more) or drops it.
    pub(crate) fn finish_pen(&mut self, cx: &mut Context<Self>) {
        let Some(pen) = self.pen.take() else { return };
        let count = self
            .pen_mask_by(&pen)
            .map(|mask| editor::read_freeform_path(&mask.params).len())
            .unwrap_or(0);
        if count >= 3 {
            let mut params = Params::new();
            params.set("closed", true);
            self.execute(
                Command::UpdateMask {
                    element_id: pen.element_id,
                    mask_id: pen.mask_id,
                    params,
                },
                cx,
            );
            self.set_status(format!("Freeform mask closed with {count} points"), cx);
        } else {
            self.execute(
                Command::RemoveMask {
                    element_id: pen.element_id,
                    mask_id: pen.mask_id,
                },
                cx,
            );
            self.set_status("Pen: a path needs at least three points; nothing was kept", cx);
        }
        cx.notify();
    }

    /// Drops the path being drawn.
    pub(crate) fn cancel_pen(&mut self, cx: &mut Context<Self>) {
        let Some(pen) = self.pen.take() else { return };
        self.execute(
            Command::RemoveMask {
                element_id: pen.element_id,
                mask_id: pen.mask_id,
            },
            cx,
        );
        self.set_status("Pen cancelled", cx);
        cx.notify();
    }

    fn pen_mask_by(&self, pen: &PenTool) -> Option<&Mask> {
        self.project()
            .element(&pen.element_id)
            .and_then(|(_, element)| element.mask(&pen.mask_id))
    }

    /// The selected clip's freeform path in canvas pixels, with whether it
    /// is closed, for the preview overlay.
    pub(crate) fn freeform_overlay(&self) -> Option<(Vec<(f64, f64)>, bool)> {
        let element = self.selected_element()?;
        let mask = element.masks.first().filter(|mask| mask.kind == Mask::FREEFORM)?;
        let quad = editor::element_quad(self.project(), element, self.playhead())?;
        let points = editor::read_freeform_path(&mask.params)
            .iter()
            .map(|point| editor::freeform_local_to_canvas(&quad, &mask.params, point.x, point.y))
            .collect();
        Some((points, mask.params.bool("closed", false)))
    }

    pub(crate) fn add_mask(&mut self, mask: Mask, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(Command::AddMask { element_id, mask }, cx);
    }

    pub(crate) fn update_mask(&mut self, mask_id: &str, params: Params, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::UpdateMask {
                element_id,
                mask_id: mask_id.to_owned(),
                params,
            },
            cx,
        );
    }

    pub(crate) fn remove_mask(&mut self, mask_id: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::RemoveMask {
                element_id,
                mask_id: mask_id.to_owned(),
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
/// The value of a parameter path at a timeline time: the element's own
/// parameters, or those of one of its effects (`effects.<id>.params.<key>`)
/// or masks (`masks.<id>.params.<key>`), keyframes included.
pub(crate) fn animated_value(element: &Element, path: &str, time: MediaTime) -> Option<f64> {
    if let Some(rest) = path.strip_prefix("effects.") {
        let (effect_id, key) = rest.split_once(".params.")?;
        let effect = element.effect(effect_id)?;
        return element.effect_params_at(effect, time).get(key)?.as_number();
    }
    if let Some(rest) = path.strip_prefix("masks.") {
        let (mask_id, key) = rest.split_once(".params.")?;
        let mask = element.mask(mask_id)?;
        return element.mask_params_at(mask, time).get(key)?.as_number();
    }
    element.params_at(time).get(path)?.as_number()
}

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


//! The open project and everything that moves around it: selection,
//! playback, the preview image, saving, thumbnails and export. Panels read
//! this entity and call its methods; it notifies them when anything changes.

/// `OPENCUT_TRACE_INPUT=1` prints scroll-wheel events and scroll offsets
/// to stderr, for driving the app from a script.
pub(crate) fn trace_input() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("OPENCUT_TRACE_INPUT").is_some_and(|v| !v.is_empty() && v != "0"))
}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use editor::{
    Command, Edge, Editor, Effect, Element, ElementPatch, Mask, MediaItem, MediaKind, Outcome,
    Params, Project, ProjectStore, TrackPlacement, nearest_free_start, plan_audio, Transition};
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

/// One point of the selected clip's freeform path for the preview overlay,
/// in canvas pixels.
#[derive(Clone, Debug)]
pub(crate) struct OverlayPoint {
    pub id: String,
    pub anchor: (f64, f64),
    pub in_handle: (f64, f64),
    pub out_handle: (f64, f64),
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FreeformOverlay {
    pub points: Vec<OverlayPoint>,
    pub closed: bool,
    /// Points can be dragged, segments clicked: a closed path that is not
    /// being drawn.
    pub editable: bool,
}

/// What a click on the preview landed on, in a freeform path.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FreeformHit {
    Anchor(String),
    /// A segment and the spot along it nearest to the click.
    Segment(usize, f64),
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
    /// Waveform peaks by media id, once the render thread has read them.
    pub peaks: HashMap<String, Arc<Vec<f32>>>,
    peaks_pending: HashSet<String>,
    /// The freeform mask being drawn on the preview, if any.
    pub pen: Option<PenTool>,
    /// Points of the selected clip's freeform mask that are selected (they
    /// are what `Delete` removes), as in the web app.
    pub selected_mask_points: Vec<String>,
    /// Platform preset whose safe area the preview shades.
    pub guide: Option<&'static editor::PlatformPreset>,
    /// The last loudness measurement: the clip's id and its report.
    pub loudness: Option<(String, editor::LoudnessReport)>,
    /// In and out marks: a stretch of the timeline to ripple-delete.
    pub mark_in: Option<MediaTime>,
    pub mark_out: Option<MediaTime>,
    pub export: Option<ExportState>,
    /// Where fetched font files live (`~/.opencut/fonts`).
    pub fonts_dir: PathBuf,
    /// Where fetched sticker SVGs live (`~/.opencut/stickers`).
    pub stickers_dir: PathBuf,
    /// Sticker ids already on disk, being fetched, or that failed.
    stickers_tried: HashSet<String>,
    render: RenderHandle,
    audio: Option<AudioOutput>,
    render_generation: u64,
    dirty: bool,
    save_task: Option<Task<()>>,
    release_task: Option<Task<()>>,
    playback_task: Option<Task<()>>,
    _event_task: Task<()>,
}

/// A new transition lasts this long (one second).
pub(crate) const DEFAULT_TRANSITION_DURATION: MediaTime = MediaTime::from_ticks(120_000);

impl EditorState {
    pub(crate) fn new(
        project: Project,
        store: ProjectStore,
        ffmpeg: Option<Arc<Ffmpeg>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let fonts_dir = crate::fonts::fonts_dir(store.root());
        let stickers_dir = crate::stickers::stickers_dir(store.root());
        let (render, mut events) = spawn_render_thread(ffmpeg.clone(), fonts_dir.clone(), stickers_dir.clone());
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
            selected_mask_points: Vec::new(),
            guide: None,
            loudness: None,
            mark_in: None,
            mark_out: None,
            peaks: HashMap::new(),
            peaks_pending: HashSet::new(),
            fonts_dir,
            stickers_dir,
            stickers_tried: HashSet::new(),
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
        if !self.selected_mask_points.is_empty() {
            let ids: Vec<String> = self
                .freeform_mask()
                .map(|(_, mask)| editor::read_freeform_path(&mask.params).into_iter().map(|point| point.id).collect())
                .unwrap_or_default();
            self.selected_mask_points.retain(|id| ids.contains(id));
        }
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
            self.selected_mask_points.clear();
        } else if !self.is_selected(element_id) {
            self.selection = vec![element_id.to_owned()];
            self.selected_mask_points.clear();
        }
        cx.notify();
    }

    pub(crate) fn select_only(&mut self, element_ids: Vec<String>, cx: &mut Context<Self>) {
        if self.pen.is_some() {
            self.finish_pen(cx);
        }
        self.selection = element_ids;
        self.selected_mask_points.clear();
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        // Escape while drawing cancels the drawing, not the selection; with
        // path points selected it drops those first (the most specific
        // selection, as in the web app).
        if self.pen.is_some() {
            self.cancel_pen(cx);
            return;
        }
        if !self.selected_mask_points.is_empty() {
            self.selected_mask_points.clear();
            cx.notify();
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
        self.selected_mask_points.clear();
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
        if !self.selected_mask_points.is_empty() {
            self.remove_selected_mask_points(cx);
            return;
        }
        if self.selection.is_empty() {
            return;
        }
        let ids = std::mem::take(&mut self.selection);
        self.execute(Command::DeleteElements(ids), cx);
    }

    /// Scans the selected video clip for shot changes with FFmpeg's scdet
    /// on a background thread and splits it at the cuts in one undo step.
    pub(crate) fn split_at_shots_selected(&mut self, threshold: f64, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        if !matches!(element.kind, editor::ElementKind::Video { .. }) {
            self.set_status("Shot detection needs a video clip.", cx);
            return;
        }
        let Some(media) = self.project().media_for(element) else { return };
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.set_status("FFmpeg is needed to look for shot changes.", cx);
            return;
        };
        let element_id = element.id.clone();
        let path = media.path.clone();
        let from = element.trim_start.to_seconds_f64();
        let to = from + element.duration.to_seconds_f64();
        let options = editor::ShotOptions { threshold, ..Default::default() };
        self.set_status("Looking for shot changes…", cx);
        cx.spawn(async move |this, cx| {
            let scored = cx
                .background_executor()
                .spawn(async move {
                    media::scene_scores(&ffmpeg, &path, from, to, threshold)
                        .map(|scores| {
                            scores
                                .into_iter()
                                .map(|score| editor::FrameScore { seconds: score.seconds, score: score.score })
                                .collect::<Vec<_>>()
                        })
                        .map_err(|error| error.to_string())
                })
                .await;
            this.update(cx, |state, cx| match scored {
                Ok(scores) => {
                    let cuts = editor::pick_cuts(&scores, &options);
                    let Some(element) = state.project().element(&element_id).map(|(_, element)| element.clone()) else {
                        return;
                    };
                    let times: Vec<MediaTime> =
                        cuts.iter().filter_map(|cut| editor::source_to_timeline(&element, cut.seconds)).collect();
                    if times.is_empty() {
                        state.set_status("No shot changes found; try a lower threshold.", cx);
                        return;
                    }
                    let count = times.len();
                    state.execute(Command::SplitElementAtTimes { element_id, times }, cx);
                    state.set_status(format!("{count} shot change(s): split into {} clips.", count + 1), cx);
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Deletes the selection and closes the gaps it leaves, in one undo step.
    pub(crate) fn ripple_delete_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.delete_selection(cx);
            return;
        }
        let ids = std::mem::take(&mut self.selection);
        self.execute(Command::RippleDelete { element_ids: ids }, cx);
    }

    // ---- marks and edit points ------------------------------------------

    pub(crate) fn set_mark_in(&mut self, cx: &mut Context<Self>) {
        let time = self.playhead();
        self.mark_in = Some(time);
        if self.mark_out.is_some_and(|out| out <= time) {
            self.mark_out = None;
        }
        cx.notify();
    }

    pub(crate) fn set_mark_out(&mut self, cx: &mut Context<Self>) {
        let time = self.playhead();
        self.mark_out = Some(time);
        if self.mark_in.is_some_and(|start| start >= time) {
            self.mark_in = None;
        }
        cx.notify();
    }

    pub(crate) fn clear_marks(&mut self, cx: &mut Context<Self>) {
        self.mark_in = None;
        self.mark_out = None;
        cx.notify();
    }

    pub(crate) fn goto_mark(&mut self, out: bool, cx: &mut Context<Self>) {
        if let Some(time) = if out { self.mark_out } else { self.mark_in } {
            self.seek(time, cx);
        }
    }

    /// Moves the playhead to the next (or previous) element start or end.
    pub(crate) fn jump_to_edit(&mut self, forward: bool, cx: &mut Context<Self>) {
        let playhead = self.playhead();
        let target = if forward {
            editor::next_edit_point(self.project(), playhead)
        } else {
            editor::previous_edit_point(self.project(), playhead)
        };
        if let Some(time) = target {
            self.seek(time, cx);
        }
    }

    /// Ripple-deletes the stretch between the in and out marks on every
    /// track, in one undo step, and clears the marks.
    pub(crate) fn delete_marked_range(&mut self, cx: &mut Context<Self>) {
        let (Some(from), Some(to)) = (self.mark_in, self.mark_out) else {
            self.set_status("Set an in and an out mark first (I and O).", cx);
            return;
        };
        if to <= from {
            return;
        }
        self.selection.clear();
        if self.execute(Command::DeleteRange { from, to }, cx).is_some() {
            self.mark_in = None;
            self.mark_out = None;
            self.seek(from, cx);
            self.set_status(format!("Removed {:.2} s between the marks.", (to - from).to_seconds_f64()), cx);
        }
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
        let (plain, keyed) = keyed_edits(element, |key| key.to_owned(), &params, time);
        for command in keyed {
            self.execute(command, cx);
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
        if let Some(value) = animated_value(element, path, time) {
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
            return;
        }
        if let Some(color) = animated_color(element, path, time) {
            self.execute(
                Command::SetColorKeyframe {
                    element_id,
                    path: path.to_owned(),
                    time,
                    color,
                },
                cx,
            );
            return;
        }
        self.set_status(format!("{path} has no value to key"), cx);
    }

    /// Ducks the selected clip under the voice: measures the other audio
    /// clips (or `voice_id` alone) on a background thread and writes the
    /// clip's volume keys in one undo step.
    pub(crate) fn duck_selected(&mut self, voice_id: Option<String>, amount_db: f64, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let music_id = element.id.clone();
        let base_db = element.params.number("volume", 0.0);
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.set_status("FFmpeg is needed to listen for speech.", cx);
            return;
        };
        let project = self.project().clone();
        let options = editor::DuckingOptions { amount_db, ..Default::default() };
        self.set_status("Listening for speech…", cx);
        cx.spawn(async move |this, cx| {
            let measured = cx
                .background_executor()
                .spawn(async move {
                    crate::ducking::measure_voice(&ffmpeg, &project, &music_id, voice_id.as_deref(), &options)
                        .map(|voice| (music_id, voice))
                })
                .await;
            this.update(cx, |state, cx| match measured {
                Ok((music_id, voice)) => {
                    let intervals = editor::speech_intervals(&voice.levels, &options, voice.clip_start);
                    let keys = editor::build_ducking_keys(
                        &intervals,
                        voice.clip_start,
                        voice.clip_duration,
                        base_db,
                        &options,
                    );
                    if keys.is_empty() {
                        state.set_status("No speech found over this clip; nothing written.", cx);
                        return;
                    }
                    let count = keys.len();
                    let keys = keys
                        .into_iter()
                        .map(|(seconds, db)| (MediaTime::from_seconds_f64(seconds).unwrap_or(MediaTime::ZERO), db))
                        .collect();
                    state.execute(
                        Command::SetKeyframes { element_id: music_id, path: "volume".to_owned(), keys },
                        cx,
                    );
                    state.set_status(
                        format!(
                            "{} speech passage(s) ({:.1} s) in {} clip(s): {count} volume keys written.",
                            intervals.len(),
                            editor::total_seconds(&intervals),
                            voice.voice_ids.len()
                        ),
                        cx,
                    );
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Cuts the silent stretches out of the selected clip: measures it on
    /// a background thread, then removes the stretches in one undo step,
    /// closing the gaps as `ripple` says.
    pub(crate) fn cut_silence_selected(
        &mut self,
        options: editor::SilenceOptions,
        ripple: editor::RippleScope,
        cx: &mut Context<Self>,
    ) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.set_status("FFmpeg is needed to listen for silence.", cx);
            return;
        };
        let project = self.project().clone();
        self.set_status("Listening for silence…", cx);
        cx.spawn(async move |this, cx| {
            let measured = cx
                .background_executor()
                .spawn(async move {
                    crate::ducking::measure_clip(&ffmpeg, &project, &element_id, options.window_seconds)
                        .map(|levels| (element_id, levels))
                })
                .await;
            this.update(cx, |state, cx| match measured {
                Ok((element_id, levels)) => {
                    let cuts = editor::silence_intervals(
                        &levels.levels,
                        levels.clip_start,
                        levels.clip_start + levels.clip_duration,
                        &options,
                    );
                    if cuts.is_empty() {
                        state.set_status("No silence long enough to cut.", cx);
                        return;
                    }
                    let removed: f64 = cuts.iter().map(|cut| cut.end - cut.start).sum();
                    let ranges = cuts
                        .iter()
                        .map(|cut| {
                            (
                                MediaTime::from_seconds_f64(cut.start).unwrap_or(MediaTime::ZERO),
                                MediaTime::from_seconds_f64(cut.end).unwrap_or(MediaTime::ZERO),
                            )
                        })
                        .collect();
                    let outcome = state.execute(Command::RemoveRanges { element_id, ranges, ripple }, cx);
                    let pieces = outcome.map(|outcome| outcome.element_refs().len()).unwrap_or(0);
                    state.set_status(
                        format!("Removed {removed:.1} s of silence in {} cut(s); {pieces} piece(s) remain.", cuts.len()),
                        cx,
                    );
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Measures the selected clip's loudness (EBU R128) on a background
    /// thread and keeps the report for the inspector.
    pub(crate) fn measure_loudness_selected(&mut self, cx: &mut Context<Self>) {
        self.run_loudness(None, cx);
    }

    /// Measures the selected clip and sets its volume so the integrated
    /// loudness meets the target (held under its true-peak ceiling).
    pub(crate) fn normalize_loudness_selected(&mut self, target_index: usize, cx: &mut Context<Self>) {
        let target = editor::LOUDNESS_TARGETS[target_index.min(editor::LOUDNESS_TARGETS.len() - 1)];
        self.run_loudness(Some(target), cx);
    }

    fn run_loudness(&mut self, target: Option<editor::LoudnessTarget>, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.set_status("FFmpeg is needed to measure loudness.", cx);
            return;
        };
        let project = self.project().clone();
        self.set_status("Measuring loudness…", cx);
        cx.spawn(async move |this, cx| {
            let measured = cx
                .background_executor()
                .spawn(async move { crate::loudness::measure_clip(&ffmpeg, &project, &element_id).map(|report| (element_id, report)) })
                .await;
            this.update(cx, |state, cx| match measured {
                Ok((id, report)) => {
                    let fmt = |value: f64, unit: &str| {
                        if value.is_finite() { format!("{value:.1} {unit}") } else { "silent".to_owned() }
                    };
                    let summary = format!(
                        "Integrated {} · range {:.1} LU · true peak {}",
                        fmt(report.integrated_lufs, "LUFS"),
                        report.loudness_range_lu,
                        fmt(report.true_peak_dbtp, "dBTP")
                    );
                    match target {
                        None => state.set_status(summary, cx),
                        Some(target) => match editor::normalization_gain(&report, target.lufs, target.true_peak_dbtp) {
                            None => state.set_status("The clip is silent; nothing to normalise.", cx),
                            Some((gain, limited)) => {
                                let volume = gain.clamp(-60.0, 20.0);
                                let mut params = editor::Params::new();
                                params.set("volume", volume);
                                state.execute(
                                    Command::UpdateElement {
                                        element_id: id.clone(),
                                        patch: editor::ElementPatch { params: Some(params), ..Default::default() },
                                    },
                                    cx,
                                );
                                state.set_status(
                                    format!(
                                        "{summary}; volume set to {volume:+.1} dB for {} LUFS{}.",
                                        target.lufs,
                                        if limited { " (held back by the true-peak ceiling)" } else { "" }
                                    ),
                                    cx,
                                );
                            }
                        },
                    }
                    state.loudness = Some((id, report));
                    cx.notify();
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
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
        let (plain, keyed) = match params {
            Some(params) => keyed_edits(element, |key| format!("effects.{effect_id}.params.{key}"), &params, time),
            None => (Params::new(), Vec::new()),
        };
        for command in keyed {
            self.execute(command, cx);
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
        let mut points = editor::read_freeform_path(&mask.params);
        let (element_id, mask_id) = (element.id.clone(), mask.id.clone());
        let mut params = Params::new();
        if points.is_empty() {
            // The first point becomes the path's centre, as in the web app.
            let origin = Params::new();
            let (cx_units, cy_units) = editor::canvas_to_freeform_local(&quad, &origin, canvas_x, canvas_y);
            points.push(editor::FreeformPoint::corner(0.0, 0.0));
            params
                .set("centerX", cx_units)
                .set("centerY", cy_units)
                .set("rotation", 0.0)
                .set("scale", 1.0)
                .set("path", editor::write_freeform_path(&points));
        } else {
            let (x, y) = editor::canvas_to_freeform_local(&quad, &mask.params, canvas_x, canvas_y);
            points.push(editor::FreeformPoint::corner(x, y));
            let (center_x, center_y, recentred) = editor::recenter_freeform_path(&quad, &mask.params, &points);
            params
                .set("centerX", center_x)
                .set("centerY", center_y)
                .set("path", editor::write_freeform_path(&recentred));
        }
        let count = points.len();
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
        let (center_x, center_y, recentred) = editor::recenter_freeform_path(&quad, &mask.params, &points);
        let (element_id, mask_id) = (element.id.clone(), mask.id.clone());
        let mut params = Params::new();
        params
            .set("centerX", center_x)
            .set("centerY", center_y)
            .set("path", editor::write_freeform_path(&recentred));
        self.execute(Command::UpdateMask { element_id, mask_id, params }, cx);
    }

    /// Closes the path (three points or more) or drops it.
    pub(crate) fn finish_pen(&mut self, cx: &mut Context<Self>) {
        let Some(pen) = self.pen.take() else { return };
        let Some(mask) = self.pen_mask_by(&pen) else {
            // Undone away while drawing: nothing to close or remove.
            cx.notify();
            return;
        };
        let count = editor::read_freeform_path(&mask.params).len();
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
        if self.pen_mask_by(&pen).is_none() {
            cx.notify();
            return;
        }
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

    /// The selected clip's freeform path in canvas pixels for the preview
    /// overlay: anchors with their handles, which are selected, and whether
    /// the path is closed and editable.
    pub(crate) fn freeform_overlay(&self) -> Option<FreeformOverlay> {
        let (element, mask) = self.freeform_mask()?;
        let quad = editor::element_quad(self.project(), element, self.playhead())?;
        let to_canvas = |x: f64, y: f64| editor::freeform_local_to_canvas(&quad, &mask.params, x, y);
        let points = editor::read_freeform_path(&mask.params)
            .iter()
            .map(|point| OverlayPoint {
                id: point.id.clone(),
                anchor: to_canvas(point.x, point.y),
                in_handle: to_canvas(point.x + point.in_x, point.y + point.in_y),
                out_handle: to_canvas(point.x + point.out_x, point.y + point.out_y),
                selected: self.selected_mask_points.contains(&point.id),
            })
            .collect();
        let closed = mask.params.bool("closed", false);
        Some(FreeformOverlay {
            points,
            closed,
            editable: closed && self.pen.is_none(),
        })
    }

    pub(crate) fn add_mask(&mut self, mask: Mask, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(Command::AddMask { element_id, mask }, cx);
    }

    /// Changes a mask's parameters; a keyframed one gets a key at the
    /// playhead instead.
    pub(crate) fn update_mask(&mut self, mask_id: &str, params: Params, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let time = self.playhead();
        let (plain, keyed) = keyed_edits(element, |key| format!("masks.{mask_id}.params.{key}"), &params, time);
        for command in keyed {
            self.execute(command, cx);
        }
        if plain.is_empty() {
            return;
        }
        self.execute(
            Command::UpdateMask {
                element_id,
                mask_id: mask_id.to_owned(),
                params: plain,
            },
            cx,
        );
    }

    /// Shows a mask with other parameters without recording an edit: the
    /// live picture while a path point is dragged. The edit is committed
    /// (or the start restored) when the drag ends.
    pub(crate) fn preview_mask_params(&mut self, mask_id: &str, params: Params, cx: &mut Context<Self>) {
        self.set_mask_params_transient(mask_id, |current| current.merge(&params));
        self.request_render(cx);
        cx.notify();
    }

    /// Puts a mask's parameters back exactly as they were before a drag
    /// (a merge would keep keys the preview added), without recording it.
    fn restore_mask_params(&mut self, mask_id: &str, params: &Params) {
        self.set_mask_params_transient(mask_id, |current| *current = params.clone());
    }

    fn set_mask_params_transient(&mut self, mask_id: &str, change: impl FnOnce(&mut Params)) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let mask_id = mask_id.to_owned();
        self.editor.apply_transient(|project| {
            let element = project
                .tracks
                .iter_mut()
                .flat_map(|track| track.elements.iter_mut())
                .find(|element| element.id == element_id);
            if let Some(element) = element
                && let Some(mask) = element.masks.iter_mut().find(|mask| mask.id == mask_id)
            {
                change(&mut mask.params);
            }
        });
    }

    /// Changes a mask's stored parameters as one edit, keys or not: what
    /// a gesture on the preview writes.
    fn update_mask_plain(&mut self, mask_id: &str, params: Params, cx: &mut Context<Self>) {
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

    // ---- freeform path editing (the web app's mask point handles) ------

    /// The selected clip and its freeform mask, if it has one.
    pub(crate) fn freeform_mask(&self) -> Option<(&Element, &Mask)> {
        let element = self.selected_element()?;
        let mask = element.masks.first().filter(|mask| mask.kind == Mask::FREEFORM)?;
        Some((element, mask))
    }

    /// What a canvas pixel lands on in the selected clip's closed path:
    /// an anchor within `tolerance` canvas pixels, else a segment within it.
    pub(crate) fn freeform_hit(&self, (x, y): (f64, f64), tolerance: f64) -> Option<FreeformHit> {
        let overlay = self.freeform_overlay()?;
        if !overlay.editable {
            return None;
        }
        let near = |(px, py): (f64, f64)| ((px - x).powi(2) + (py - y).powi(2)).sqrt() <= tolerance;
        if let Some(point) = overlay.points.iter().find(|point| near(point.anchor)) {
            return Some(FreeformHit::Anchor(point.id.clone()));
        }
        let (element, mask) = self.freeform_mask()?;
        let quad = editor::element_quad(self.project(), element, self.playhead())?;
        let points = editor::read_freeform_path(&mask.params);
        let mut best: Option<(usize, f64, f64)> = None;
        for segment in 0..editor::freeform_segment_count(&points, true) {
            if let Some((t, distance)) = editor::closest_on_freeform_segment(&quad, &mask.params, &points, true, segment, (x, y))
                && distance <= tolerance
                && best.is_none_or(|(_, _, nearest)| distance < nearest)
            {
                best = Some((segment, t, distance));
            }
        }
        best.map(|(segment, t, _)| FreeformHit::Segment(segment, t))
    }

    pub(crate) fn select_mask_point(&mut self, point_id: &str, extend: bool, cx: &mut Context<Self>) {
        if extend {
            if let Some(index) = self.selected_mask_points.iter().position(|id| id == point_id) {
                self.selected_mask_points.remove(index);
            } else {
                self.selected_mask_points.push(point_id.to_owned());
            }
        } else {
            self.selected_mask_points = vec![point_id.to_owned()];
        }
        cx.notify();
    }

    pub(crate) fn clear_mask_point_selection(&mut self, cx: &mut Context<Self>) {
        if !self.selected_mask_points.is_empty() {
            self.selected_mask_points.clear();
            cx.notify();
        }
    }

    /// The path's parameters with one anchor moved to a canvas pixel,
    /// computed from the parameters the drag started with so repeated
    /// moves do not accumulate.
    fn moved_point_params(&self, start: &Params, point_id: &str, (x, y): (f64, f64)) -> Option<Params> {
        let (element, _) = self.freeform_mask()?;
        let quad = editor::element_quad(self.project(), element, self.playhead())?;
        let (lx, ly) = editor::canvas_to_freeform_local(&quad, start, x, y);
        let mut points = editor::read_freeform_path(start);
        let point = points.iter_mut().find(|point| point.id == point_id)?;
        point.x = lx;
        point.y = ly;
        let mut params = Params::new();
        params.set("path", editor::write_freeform_path(&points));
        Some(params)
    }

    /// The mask's centre shifted by a canvas-pixel delta (dragging a segment
    /// moves the whole path, as in the web app).
    fn shifted_center_params(&self, start: &Params, (dx, dy): (f64, f64)) -> Option<Params> {
        let (element, _) = self.freeform_mask()?;
        let quad = editor::element_quad(self.project(), element, self.playhead())?;
        if quad.width <= 0.0 || quad.height <= 0.0 {
            return None;
        }
        // Into the element's own (unrotated) frame, in units of its size.
        let (sin, cos) = (-quad.rotation_degrees).to_radians().sin_cos();
        let (ex, ey) = (dx * cos - dy * sin, dx * sin + dy * cos);
        let mut params = Params::new();
        params
            .set("centerX", start.number("centerX", 0.0) + ex / quad.width)
            .set("centerY", start.number("centerY", 0.0) + ey / quad.height);
        Some(params)
    }

    /// Previews or commits a dragged anchor (`commit` restores the start
    /// first, so the edit undoes in one step).
    pub(crate) fn drag_mask_point(&mut self, start: &Params, point_id: &str, to: (f64, f64), commit: bool, cx: &mut Context<Self>) {
        let Some(params) = self.moved_point_params(start, point_id, to) else { return };
        self.apply_drag(start, params, commit, cx);
    }

    /// Previews or commits a dragged segment: the whole path moves.
    pub(crate) fn drag_mask_path(&mut self, start: &Params, delta: (f64, f64), commit: bool, cx: &mut Context<Self>) {
        let Some(params) = self.shifted_center_params(start, delta) else { return };
        self.apply_drag(start, params, commit, cx);
    }

    fn apply_drag(&mut self, start: &Params, params: Params, commit: bool, cx: &mut Context<Self>) {
        let Some((_, mask)) = self.freeform_mask() else { return };
        let mask_id = mask.id.clone();
        if commit {
            // Back to the start, so the edit's undo snapshot is the pre-drag path.
            self.restore_mask_params(&mask_id, start);
            self.update_mask_plain(&mask_id, params, cx);
        } else {
            self.preview_mask_params(&mask_id, params, cx);
        }
    }

    /// Adds a point on a segment at `t` (a click on the path), recentres
    /// the path as the web app does, and selects the new point.
    pub(crate) fn insert_mask_point(&mut self, segment: usize, t: f64, cx: &mut Context<Self>) {
        let Some((element, mask)) = self.freeform_mask() else { return };
        let Some(quad) = editor::element_quad(self.project(), element, self.playhead()) else { return };
        let points = editor::read_freeform_path(&mask.params);
        let inserted = editor::insert_freeform_point(&points, true, segment, t);
        if inserted.len() == points.len() {
            return;
        }
        let new_id = inserted
            .iter()
            .find(|point| !points.iter().any(|old| old.id == point.id))
            .map(|point| point.id.clone());
        let (center_x, center_y, recentred) = editor::recenter_freeform_path(&quad, &mask.params, &inserted);
        let mask_id = mask.id.clone();
        let mut params = Params::new();
        params
            .set("path", editor::write_freeform_path(&recentred))
            .set("centerX", center_x)
            .set("centerY", center_y);
        self.update_mask_plain(&mask_id, params, cx);
        if let Some(id) = new_id {
            self.selected_mask_points = vec![id];
        }
        self.set_status(format!("Point added: {} points", recentred.len()), cx);
    }

    /// Drops the selected points; a path left with fewer than three
    /// points opens (and so stops masking), as in the web app.
    pub(crate) fn remove_selected_mask_points(&mut self, cx: &mut Context<Self>) {
        let ids = std::mem::take(&mut self.selected_mask_points);
        let Some((_, mask)) = self.freeform_mask() else { return };
        let points = editor::read_freeform_path(&mask.params);
        let closed = mask.params.bool("closed", false);
        let (kept, still_closed) = editor::remove_freeform_points(&points, &ids, closed);
        if kept.len() == points.len() {
            return;
        }
        let mask_id = mask.id.clone();
        let mut params = Params::new();
        params.set("path", editor::write_freeform_path(&kept)).set("closed", still_closed);
        self.update_mask_plain(&mask_id, params, cx);
        self.set_status(
            if still_closed || !closed {
                format!("{} point(s) removed", points.len() - kept.len())
            } else {
                "Point removed; a path needs three points to mask, so it is open now".to_owned()
            },
            cx,
        );
    }

    // ---- transitions -----------------------------------------------------

    /// Gives the selected clip a transition of `kind` (a catalogue name),
    /// keeping its duration and parameters when it already has one of
    /// another kind, else the default of one second.
    pub(crate) fn set_transition_kind(&mut self, kind: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        let duration = element
            .transition
            .as_ref()
            .map(|transition| transition.duration)
            .unwrap_or(DEFAULT_TRANSITION_DURATION);
        let transition = Transition {
            kind: kind.to_owned(),
            duration,
            params: Params::new(),
        };
        self.execute(
            Command::SetTransition {
                element_id,
                transition: Some(transition),
            },
            cx,
        );
    }

    /// Changes the selected clip's transition duration (seconds) or
    /// parameters.
    pub(crate) fn update_transition(&mut self, duration: Option<MediaTime>, params: Option<Params>, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let Some(current) = element.transition.as_ref() else { return };
        let element_id = element.id.clone();
        let mut transition = current.clone();
        if let Some(duration) = duration {
            transition.duration = duration;
        }
        if let Some(params) = params {
            transition.params.merge(&params);
        }
        self.execute(
            Command::SetTransition {
                element_id,
                transition: Some(transition),
            },
            cx,
        );
    }

    pub(crate) fn remove_transition(&mut self, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let element_id = element.id.clone();
        self.execute(
            Command::SetTransition {
                element_id,
                transition: None,
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
        // The picture follows the audio device's clock while sound plays,
        // so the two cannot drift; without audio, the wall clock.
        let time = self
            .audio
            .as_ref()
            .and_then(|audio| audio.position_seconds())
            .and_then(MediaTime::from_seconds_f64)
            .unwrap_or_else(|| {
                let elapsed = MediaTime::from_seconds_f64(anchor_instant.elapsed().as_secs_f64())
                    .unwrap_or(MediaTime::ZERO);
                anchor_time + elapsed
            });
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
            RenderEvent::Peaks { media_id, peaks } => {
                self.peaks_pending.remove(&media_id);
                self.peaks.insert(media_id, peaks);
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

    /// Asks the render thread for the waveform peaks of media files the
    /// timeline wants to draw and does not have yet.
    pub(crate) fn request_peaks(&mut self, media_ids: Vec<String>) {
        for media_id in media_ids {
            if self.peaks.contains_key(&media_id) || self.peaks_pending.contains(&media_id) {
                continue;
            }
            let Some(media) = self.project().media_item(&media_id).cloned() else { continue };
            self.peaks_pending.insert(media_id);
            self.render.send(RenderRequest::Peaks { media });
        }
    }

    /// Transcribes the selected clip with whisper.cpp on a background thread
    /// (fetching the model first when needed) and adds the captions as a
    /// text track in one undo step.
    pub(crate) fn transcribe_selected(&mut self, model_id: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected_element() else { return };
        let Some(clip) = crate::transcribe::clip_plan(self.project(), &element.id) else {
            self.set_status("Select a clip with audio to transcribe.", cx);
            return;
        };
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.set_status("FFmpeg is needed to read the clip's audio.", cx);
            return;
        };
        let Some(model) = crate::transcribe::model_spec(model_id) else { return };
        let tools = crate::transcribe::tools_dir(self.store.root());
        let Some(cli) = crate::transcribe::whisper_cli(&tools) else {
            self.set_status(
                format!(
                    "whisper-cli was not found. Build it with scripts/build-whisper-cli.sh (CMake and a C++ toolchain) or set OPENCUT_WHISPER_CLI; looked in {}",
                    tools.display()
                ),
                cx,
            );
            return;
        };
        let models = crate::transcribe::models_dir(self.store.root());
        let canvas = self.project().settings.canvas;
        self.set_status(format!("Transcribing with the {} model…", model.id), cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let path = crate::transcribe::ensure_model(&models, model, &|_, _| {})?;
                    let samples = crate::transcribe::clip_samples(&ffmpeg, &clip)?;
                    let words = crate::transcribe::transcribe(&cli, model, &path, &samples, None, clip.timeline_start.to_seconds_f64())?;
                    let chunks = editor::build_caption_chunks(
                        &words,
                        editor::DEFAULT_WORDS_PER_CAPTION,
                        editor::MIN_CAPTION_DURATION_SECONDS,
                        editor::MAX_CAPTION_WORD_GAP_SECONDS,
                    );
                    let elements: Vec<editor::Element> = chunks
                        .iter()
                        .enumerate()
                        .map(|(index, chunk)| editor::caption_element(index, chunk, canvas))
                        .collect();
                    Ok::<_, String>((words.len(), elements))
                })
                .await;
            this.update(cx, |state, cx| match result {
                Ok((word_count, elements)) if elements.is_empty() => {
                    state.set_status(format!("No speech found ({word_count} words)."), cx);
                }
                Ok((word_count, elements)) => {
                    let count = elements.len();
                    state.execute(Command::InsertCaptions { elements }, cx);
                    state.set_status(format!("{count} caption(s) from {word_count} words added on a Captions track."), cx);
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Fetches the SVG of every sticker in the project that is not on disk
    /// yet, on a background thread, then renders again. Cheap to call often.
    pub(crate) fn ensure_stickers(&mut self, cx: &mut Context<Self>) {
        let missing = crate::stickers::missing_stickers(self.project(), &self.stickers_dir, &self.stickers_tried);
        if missing.is_empty() {
            return;
        }
        for id in &missing {
            self.stickers_tried.insert(id.clone());
        }
        let dir = self.stickers_dir.clone();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    missing
                        .into_iter()
                        .map(|id| (id.clone(), crate::stickers::fetch_sticker(&dir, &id)))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |state, cx| {
                let failures: Vec<String> = results
                    .into_iter()
                    .filter_map(|(id, result)| result.err().map(|message| format!("{id}: {message}")))
                    .collect();
                if !failures.is_empty() {
                    state.set_status(format!("Sticker(s) not fetched: {}", failures.join("; ")), cx);
                }
                state.request_render(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Fetches a family's files from Fontsource into the fonts directory
    /// on a background thread, then has the renderer lay text out again.
    pub(crate) fn fetch_font(&mut self, family: String, cx: &mut Context<Self>) {
        let fonts_dir = self.fonts_dir.clone();
        self.set_status(format!("Fetching {family} from Fontsource…"), cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::fonts::fetch_family(&fonts_dir, &family) })
                .await;
            this.update(cx, |state, cx| match result {
                Ok(fetched) => {
                    state.render.send(RenderRequest::ReloadFonts);
                    state.set_status(
                        format!(
                            "{} ({}): {} file(s) in {}",
                            fetched.family,
                            fetched.license,
                            fetched.files,
                            fetched.dir.display()
                        ),
                        cx,
                    );
                    state.request_render(cx);
                }
                Err(message) => state.set_status(message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Shades a platform's safe area over the preview, or clears it.
    pub(crate) fn set_guide(&mut self, guide: Option<&'static editor::PlatformPreset>, cx: &mut Context<Self>) {
        self.guide = guide;
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
/// The parameters of `params` that are keyframed, as key-setting commands
/// at `time` (numbers and hex colours), and the rest as a plain patch.
fn keyed_edits(
    element: &Element,
    path_of: impl Fn(&str) -> String,
    params: &Params,
    time: MediaTime,
) -> (Params, Vec<Command>) {
    let mut plain = Params::new();
    let mut keyed = Vec::new();
    for (key, value) in params.iter() {
        let path = path_of(key);
        if !element.is_animated(&path) {
            plain.set(key.clone(), value.clone());
            continue;
        }
        // By the value's type: "000000" typed into a colour field is a colour.
        match value {
            editor::ParamValue::Number(number) => keyed.push(Command::SetKeyframe {
                element_id: element.id.clone(),
                path,
                time,
                value: *number,
                segment: None,
            }),
            editor::ParamValue::Text(text) if editor::parse_hex_color(text).is_some() => {
                keyed.push(Command::SetColorKeyframe {
                    element_id: element.id.clone(),
                    path,
                    time,
                    color: text.trim().to_owned(),
                })
            }
            _ => {
                plain.set(key.clone(), value.clone());
            }
        }
    }
    (plain, keyed)
}

/// The hex colour a keyframe path shows at a timeline time, if it is one.
pub(crate) fn animated_color(element: &Element, path: &str, time: MediaTime) -> Option<String> {
    let text = if let Some(rest) = path.strip_prefix("effects.") {
        let (effect_id, key) = rest.split_once(".params.")?;
        let effect = element.effect(effect_id)?;
        element.effect_params_at(effect, time).get(key)?.as_text()
    } else if let Some(rest) = path.strip_prefix("masks.") {
        let (mask_id, key) = rest.split_once(".params.")?;
        let mask = element.mask(mask_id)?;
        element.mask_params_at(mask, time).get(key)?.as_text()
    } else {
        element.params_at(time).get(path)?.as_text()
    };
    editor::parse_hex_color(&text).is_some().then(|| text.trim().to_owned())
}

/// The number a keyframe path shows at a timeline time; text (a colour
/// such as "112233") is never read as a number.
pub(crate) fn animated_value(element: &Element, path: &str, time: MediaTime) -> Option<f64> {
    let number = |value: Option<&editor::ParamValue>| match value? {
        editor::ParamValue::Number(number) => Some(*number),
        _ => None,
    };
    if let Some(rest) = path.strip_prefix("effects.") {
        let (effect_id, key) = rest.split_once(".params.")?;
        let effect = element.effect(effect_id)?;
        return number(element.effect_params_at(effect, time).get(key));
    }
    if let Some(rest) = path.strip_prefix("masks.") {
        let (mask_id, key) = rest.split_once(".params.")?;
        let mask = element.mask(mask_id)?;
        return number(element.mask_params_at(mask, time).get(key));
    }
    number(element.params_at(time).get(path))
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


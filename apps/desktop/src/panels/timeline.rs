use std::cell::Cell;
use std::rc::Rc;

use std::collections::HashMap;
use std::sync::Arc;

use editor::{Edge, Element, ElementKind, TrackKind};
use gpui::{
    Bounds, Context, DragMoveEvent, Entity, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, Window, canvas, div, fill, hsla, point, prelude::*, px, size,
};
use time::MediaTime;

use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::format::{short_duration, timecode};
use crate::state::EditorState;
use crate::theme::ActiveTheme;

const HEADER_WIDTH: f32 = 150.0;
const RULER_HEIGHT: f32 = 24.0;
const TRACK_HEIGHT: f32 = 52.0;
const HANDLE_WIDTH: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragMode {
    Move,
    TrimStart,
    TrimEnd,
    Scrub,
}

/// What is being dragged, created by GPUI's drag system on mouse down.
#[derive(Clone, Debug)]
struct TimelineDrag {
    mode: DragMode,
    element_id: String,
    /// Seconds between the pointer and the clip's start when the drag began.
    grab_offset: f64,
}

/// Live state of the drag, updated on every move and committed on release.
#[derive(Clone, Debug)]
struct DragPreview {
    drag: TimelineDrag,
    /// Clip start (move) or edge time (trim), in seconds.
    time: f64,
    track_index: Option<usize>,
    moved: bool,
}

pub(crate) struct TimelinePanel {
    state: Entity<EditorState>,
    /// Bounds of the rows area (headers + lanes), from the last layout.
    rows_bounds: Rc<Cell<Bounds<Pixels>>>,
    preview: Option<DragPreview>,
    /// Seconds between the pointer and the clip start at the last mouse
    /// down on a clip; a drag that follows starts from it.
    pending_grab: f64,
}

impl TimelinePanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            rows_bounds: Rc::new(Cell::new(Bounds::default())),
            preview: None,
            pending_grab: 0.0,
        }
    }

    fn lane_origin_x(&self) -> Pixels {
        self.rows_bounds.get().left() + px(HEADER_WIDTH)
    }

    fn seconds_at(&self, x: Pixels, cx: &Context<Self>) -> f64 {
        let state = self.state.read(cx);
        let offset = f32::from(x - self.lane_origin_x()).max(0.0);
        state.scroll_seconds + offset as f64 / state.zoom as f64
    }

    fn track_index_at(&self, y: Pixels) -> Option<usize> {
        let top = self.rows_bounds.get().top() + px(RULER_HEIGHT);
        let offset = f32::from(y - top);
        if offset < 0.0 {
            return None;
        }
        Some((offset / TRACK_HEIGHT) as usize)
    }

    fn scrub_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let seconds = self.seconds_at(x, cx);
        let time = MediaTime::from_seconds_f64(seconds).unwrap_or(MediaTime::ZERO);
        self.state.update(cx, |state, cx| {
            let rate = state.project().settings.fps;
            let snapped = time.round_to_frame(rate).unwrap_or(time);
            state.seek(snapped, cx);
        });
    }

    fn on_drag_move(&mut self, event: &DragMoveEvent<TimelineDrag>, _window: &mut Window, cx: &mut Context<Self>) {
        let mut drag = event.drag(cx).clone();
        if let Some(preview) = &self.preview
            && preview.drag.element_id == drag.element_id
        {
            // GPUI hands back the value given to `on_drag`; the grab offset
            // computed on mouse down lives in the preview.
            drag.grab_offset = preview.drag.grab_offset;
        }
        let position = event.event.position;
        match drag.mode {
            DragMode::Scrub => {
                self.scrub_to(position.x, cx);
            }
            DragMode::Move => {
                let pointer_seconds = self.seconds_at(position.x, cx);
                let mut start = (pointer_seconds - drag.grab_offset).max(0.0);
                // Snap the clip's start or end to a beat marker within 8 px.
                {
                    let state = self.state.read(cx);
                    if !state.markers.is_empty() {
                        let tolerance = 8.0 / state.zoom.max(1.0) as f64;
                        let duration = state
                            .project()
                            .element(&drag.element_id)
                            .map(|(_, element)| element.duration.to_seconds_f64())
                            .unwrap_or(0.0);
                        start = snap_to_markers(start, duration, &state.markers, tolerance);
                    }
                }
                let track_index = self.track_index_at(position.y);
                let moved = self
                    .preview
                    .as_ref()
                    .map(|preview| preview.moved || (preview.time - start).abs() > 1e-6)
                    .unwrap_or(false);
                self.preview = Some(DragPreview {
                    drag,
                    time: start,
                    track_index,
                    moved,
                });
                cx.notify();
            }
            DragMode::TrimStart | DragMode::TrimEnd => {
                let time = self.seconds_at(position.x, cx).max(0.0);
                self.preview = Some(DragPreview {
                    drag,
                    time,
                    track_index: None,
                    moved: true,
                });
                cx.notify();
            }
        }
    }

    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.take() else {
            return;
        };
        cx.notify();
        if !preview.moved {
            return;
        }
        let element_id = preview.drag.element_id.clone();
        let time = MediaTime::from_seconds_f64(preview.time).unwrap_or(MediaTime::ZERO);
        self.state.update(cx, |state, cx| {
            let rate = state.project().settings.fps;
            let time = time.round_to_frame(rate).unwrap_or(time);
            match preview.drag.mode {
                DragMode::Move => {
                    let track_id = preview
                        .track_index
                        .and_then(|index| state.project().tracks.get(index))
                        .map(|track| track.id.clone());
                    state.move_element(&element_id, track_id.as_deref(), time, cx);
                }
                DragMode::TrimStart => state.resize_element(&element_id, Edge::Start, time, cx),
                DragMode::TrimEnd => state.resize_element(&element_id, Edge::End, time, cx),
                DragMode::Scrub => {}
            }
        });
    }

    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(window.line_height());
        if crate::state::trace_input() {
            eprintln!("trace: timeline wheel delta={:?} modifiers={:?}", delta, event.modifiers);
        }
        let dx = f32::from(delta.x);
        let dy = f32::from(delta.y);
        if event.modifiers.control || event.modifiers.platform {
            let anchor = self.seconds_at(event.position.x, cx);
            let factor = if dy > 0.0 { 1.15 } else { 1.0 / 1.15 };
            self.state
                .update(cx, |state, cx| state.zoom_by(factor, Some(anchor), cx));
        } else if event.modifiers.shift || dx.abs() > dy.abs() {
            let amount = if dx.abs() > dy.abs() { dx } else { dy };
            self.state
                .update(cx, |state, cx| state.scroll_by_pixels(-amount, cx));
        }
    }

    fn clip_color(kind: &ElementKind) -> Hsla {
        match kind {
            ElementKind::Video { .. } => hsla(0.58, 0.55, 0.45, 1.0),
            ElementKind::Image { .. } => hsla(0.40, 0.45, 0.40, 1.0),
            ElementKind::Text => hsla(0.08, 0.70, 0.50, 1.0),
            ElementKind::Audio { .. } => hsla(0.75, 0.45, 0.50, 1.0),
            ElementKind::Sticker { .. } => hsla(0.48, 0.50, 0.42, 1.0),
        }
    }

    fn ruler_step(zoom: f32) -> f64 {
        const STEPS: [f64; 13] = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0];
        STEPS
            .into_iter()
            .find(|step| step * zoom as f64 >= 70.0)
            .unwrap_or(600.0)
    }
}

impl Render for TimelinePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;
        let state_entity = self.state.clone();
        let bounds = self.rows_bounds.get();
        let lane_width = (f32::from(bounds.size.width) - HEADER_WIDTH).max(1.0);
        // Keep the playhead on screen while playing.
        if self.state.read(cx).playing {
            self.state.update(cx, |state, cx| state.follow_playhead(lane_width, cx));
        }
        let state = self.state.read(cx);
        let project = state.project();
        let rate = project.settings.fps;
        let zoom = state.zoom;
        let scroll = state.scroll_seconds;
        let playhead = state.playhead();
        let duration = state.duration();
        let marks = (state.mark_in, state.mark_out);
        let markers: Vec<f64> = state.markers.iter().map(|time| time.to_seconds_f64()).collect();
        // Saved markers: start, length and colour.
        let saved_markers: Vec<(f64, f64, gpui::Hsla)> = project
            .markers
            .iter()
            .map(|marker| {
                (
                    marker.time.to_seconds_f64(),
                    marker.duration.map(|length| length.to_seconds_f64()).unwrap_or(0.0),
                    marker_color(marker.color.as_deref()),
                )
            })
            .collect();
        let can_undo = state.editor.can_undo();
        let can_redo = state.editor.can_redo();
        let selection = state.selection.clone();
        let peaks_by_media: HashMap<String, Arc<Vec<f32>>> = state.peaks.clone();
        let mut peaks_wanted: Vec<String> = Vec::new();
        let status = state.status.clone();
        let focus_handle = state.focus_handle.clone();
        let preview = self.preview.clone();

        let to_x = |seconds: f64| -> f32 { ((seconds - scroll) * zoom as f64) as f32 };

        // --- ruler -------------------------------------------------------
        let step = Self::ruler_step(zoom);
        let first_tick = (scroll / step).floor() as i64;
        let visible_seconds = lane_width as f64 / zoom as f64;
        let last_tick = ((scroll + visible_seconds) / step).ceil() as i64;
        let ticks = (first_tick..=last_tick).map(|index| {
            let seconds = index as f64 * step;
            let label = if step < 1.0 {
                format!("{seconds:.2}")
            } else {
                short_duration(seconds)
            };
            div()
                .absolute()
                .top_0()
                .left(px(to_x(seconds)))
                .h_full()
                .border_l_1()
                .border_color(colors.border)
                .pl(px(3.))
                .text_size(px(10.))
                .text_color(colors.muted_foreground)
                .whitespace_nowrap()
                .child(label)
        });
        let focus_for_ruler = focus_handle.clone();
        let ruler = div()
            .flex()
            .h(px(RULER_HEIGHT))
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex_none()
                    .w(px(HEADER_WIDTH))
                    .h_full()
                    .border_r_1()
                    .border_color(colors.border)
                    .bg(colors.card),
            )
            .child(
                div()
                    .id("ruler")
                    .relative()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            window.focus(&focus_for_ruler);
                            this.scrub_to(event.position.x, cx);
                        }),
                    )
                    .on_drag(
                        TimelineDrag {
                            mode: DragMode::Scrub,
                            element_id: String::new(),
                            grab_offset: 0.0,
                        },
                        |_, _, _, cx| cx.new(|_| gpui::Empty),
                    )
                    .children(ticks)
                    .children(markers.iter().filter(|seconds| **seconds >= scroll && **seconds <= scroll + visible_seconds).map(|seconds| {
                        div()
                            .absolute()
                            .bottom_0()
                            .left(px(to_x(*seconds) - 1.0))
                            .w(px(2.))
                            .h(px(8.))
                            .bg(colors.primary)
                    }))
                    .children(
                        saved_markers
                            .iter()
                            .filter(|(start, length, _)| start + length >= scroll && *start <= scroll + visible_seconds)
                            .map(|(start, length, color)| {
                                let left = to_x(*start);
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(px(left - 1.0))
                                    .h_full()
                                    .w(px((to_x(start + length) - left).max(0.0) + 2.0))
                                    .border_l_2()
                                    .border_color(*color)
                                    .when(*length > 0.0, |this| this.bg(color.opacity(0.18)))
                                    .child(div().size(px(7.)).bg(*color).rounded(px(1.)))
                            }),
                    )
                    .when(marks.0.is_some() || marks.1.is_some(), |this| {
                        let start = marks.0.map(|t| t.to_seconds_f64()).unwrap_or(0.0);
                        let end = marks.1.map(|t| t.to_seconds_f64()).unwrap_or(duration.to_seconds_f64().max(start));
                        let left = to_x(start);
                        let width = (to_x(end) - left).max(0.0);
                        this.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(left))
                                .w(px(width))
                                .h_full()
                                .bg(colors.primary.opacity(0.15))
                                .when(marks.0.is_some(), |this| this.border_l_2().border_color(colors.primary))
                                .when(marks.1.is_some(), |this| this.border_r_2().border_color(colors.primary)),
                        )
                    }),
            );

        // --- tracks ------------------------------------------------------
        let mut rows = Vec::new();
        for (track_index, track) in project.tracks.iter().enumerate() {
            let track_id = track.id.clone();
            let track_label = format!("{}", track.name);
            let kind_label = match track.kind {
                TrackKind::Video => "video",
                TrackKind::Text => "text",
                TrackKind::Audio => "audio",
            };
            let dimmed = track.hidden || track.muted;
            let locked = track.locked;

            // Elements shown on this lane: its own (except one being moved
            // away) plus a clip being dragged onto it.
            let mut shown: Vec<(Element, bool)> = Vec::new();
            for element in &track.elements {
                if let Some(preview) = &preview
                    && preview.drag.element_id == element.id
                    && preview.moved
                {
                    match preview.drag.mode {
                        DragMode::Move => {
                            let target = preview.track_index.unwrap_or(track_index);
                            let target_ok = project
                                .tracks
                                .get(target)
                                .map(|t| t.accepts(&element.kind))
                                .unwrap_or(false);
                            let target = if target_ok { target } else { track_index };
                            if target == track_index {
                                let mut ghost = element.clone();
                                ghost.start_time = MediaTime::from_seconds_f64(preview.time)
                                    .unwrap_or(element.start_time);
                                shown.push((ghost, true));
                            }
                            continue;
                        }
                        DragMode::TrimStart => {
                            let mut ghost = element.clone();
                            let end = element.end_time();
                            let start = MediaTime::from_seconds_f64(preview.time)
                                .unwrap_or(element.start_time)
                                .clamp(
                                    (element.start_time - element.trim_start).max(MediaTime::ZERO),
                                    end - MediaTime::from_ticks(1200),
                                );
                            ghost.start_time = start;
                            ghost.duration = end - start;
                            shown.push((ghost, true));
                            continue;
                        }
                        DragMode::TrimEnd => {
                            let mut ghost = element.clone();
                            let max_end = element
                                .max_duration()
                                .map(|max| element.start_time + max)
                                .unwrap_or(MediaTime::from_ticks(i64::MAX / 4));
                            let end = MediaTime::from_seconds_f64(preview.time)
                                .unwrap_or(element.end_time())
                                .clamp(element.start_time + MediaTime::from_ticks(1200), max_end);
                            ghost.duration = end - element.start_time;
                            shown.push((ghost, true));
                            continue;
                        }
                        DragMode::Scrub => {}
                    }
                }
                shown.push((element.clone(), false));
            }
            if let Some(preview) = &preview
                && preview.drag.mode == DragMode::Move
                && preview.moved
                && let Some((from_track, element)) = project.element(&preview.drag.element_id)
                && from_track.id != track.id
            {
                let target = preview.track_index.unwrap_or(usize::MAX);
                if target == track_index && track.accepts(&element.kind) {
                    let mut ghost = element.clone();
                    ghost.start_time =
                        MediaTime::from_seconds_f64(preview.time).unwrap_or(element.start_time);
                    shown.push((ghost, true));
                }
            }

            let clips = shown.into_iter().map(|(element, is_ghost)| {
                let left = to_x(element.start_time.to_seconds_f64());
                let width = (element.duration.to_seconds_f64() * zoom as f64).max(2.0) as f32;
                let selected = selection.iter().any(|id| *id == element.id);
                let color = Self::clip_color(&element.kind);
                let id = element.id.clone();
                let label = element.name.clone();
                let detail = match &element.kind {
                    ElementKind::Text => element.params.text("content", "").into_owned(),
                    _ => timecode(element.duration, rate),
                };
                let select_id = id.clone();
                let focus_for_clip = focus_handle.clone();
                let start_seconds = element.start_time.to_seconds_f64();

                // Audio on the clip: its peaks, the gain curve and mute.
                let audio_media = match &element.kind {
                    ElementKind::Audio { media_id } | ElementKind::Video { media_id } => project
                        .media_item(media_id)
                        .filter(|media| media.has_audio)
                        .map(|media| media.id.clone()),
                    _ => None,
                };
                let waveform = audio_media.as_ref().and_then(|media_id| match peaks_by_media.get(media_id) {
                    Some(peaks) => Some(Waveform {
                        peaks: Arc::clone(peaks),
                        source_start: element.trim_start.to_seconds_f64(),
                        zoom,
                        static_db: element.params.number("volume", 0.0),
                        volume: element.animations.get("volume").cloned(),
                        muted: element.params.bool("muted", false) || dimmed,
                    }),
                    None => {
                        peaks_wanted.push(media_id.clone());
                        None
                    }
                });
                let muted_badge = element.params.bool("muted", false);

                // Keyframe markers: one dot per distinct key time, on the
                // selected clip only.
                let mut key_times: Vec<MediaTime> = if selected {
                    element
                        .animations
                        .values()
                        .flat_map(|channel| channel.all_keys().map(|key| key.time))
                        .filter(|&time| time >= MediaTime::ZERO && time <= element.duration)
                        .collect()
                } else {
                    Vec::new()
                };
                key_times.sort();
                key_times.dedup();
                // The part of a transition that plays over this clip: the
                // end of a clip that has one, the start of the clip after it.
                let mut spans: Vec<(MediaTime, MediaTime, bool)> = Vec::new();
                if let Some(window) = editor::window_of(track, &element) {
                    spans.push((window.start.max(element.start_time), window.end.min(element.end_time()), true));
                }
                if let Some(window) = editor::window_into(track, &element) {
                    spans.push((window.start.max(element.start_time), window.end.min(element.end_time()), false));
                }
                let transition_marks: Vec<_> = spans
                    .into_iter()
                    .filter(|(start, end, _)| end > start)
                    .map(|(start, end, outgoing)| {
                        let x = ((start - element.start_time).to_seconds_f64() * zoom as f64) as f32;
                        let w = ((end - start).to_seconds_f64() * zoom as f64).max(2.0) as f32;
                        div()
                            .absolute()
                            .top_0()
                            .left(px(x))
                            .w(px(w))
                            .h_full()
                            .bg(gpui::white().opacity(0.22))
                            .when(outgoing, |this| this.border_l_1().border_color(gpui::white().opacity(0.7)))
                            .when(!outgoing, |this| this.border_r_1().border_color(gpui::white().opacity(0.7)))
                    })
                    .collect();
                let markers: Vec<_> = key_times
                    .into_iter()
                    .map(|time| {
                        let x = (time.to_seconds_f64() * zoom as f64) as f32;
                        let target = element.start_time + time;
                        let state = state_entity.clone();
                        let at_playhead = (target - playhead).as_ticks().abs() <= editor::KEY_SNAP.as_ticks();
                        div()
                            .id(eid("key", &format!("{id}:{}", time.as_ticks())))
                            .absolute()
                            .top(px(TRACK_HEIGHT / 2.0 - 8.0))
                            .left(px(x - 4.0))
                            .size(px(8.))
                            .rounded_full()
                            .bg(if at_playhead { colors.foreground } else { gpui::white().opacity(0.75) })
                            .border_1()
                            .border_color(gpui::black().opacity(0.6))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    state.update(cx, |state, cx| state.seek(target, cx));
                                },
                            )
                    })
                    .collect();

                div()
                    .id(eid("clip", &id))
                    .absolute()
                    .top(px(4.))
                    .left(px(left))
                    .w(px(width))
                    .h(px(TRACK_HEIGHT - 8.0))
                    .rounded(px(4.))
                    .bg(color.opacity(if is_ghost { 0.5 } else { 0.85 }))
                    .border_2()
                    .border_color(if selected { colors.foreground } else { color.opacity(0.3) })
                    .when(element.hidden, |this| this.opacity(0.45))
                    .when(locked, |this| this.opacity(0.5))
                    .overflow_hidden()
                    .when(!locked, |this| this.cursor_grab())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            // Keep the lane underneath from clearing the selection and scrubbing.
                            cx.stop_propagation();
                            window.focus(&focus_for_clip);
                            let extend = event.modifiers.shift;
                            this.pending_grab = this.seconds_at(event.position.x, cx) - start_seconds;
                            this.state.update(cx, |state, cx| state.select(&select_id, extend, cx));
                        }),
                    )
                    .when(!locked, |this| this.on_drag(
                        TimelineDrag {
                            mode: DragMode::Move,
                            element_id: id.clone(),
                            grab_offset: 0.0,
                        },
                        {
                            let this = cx.entity().clone();
                            move |drag: &TimelineDrag, _offset: Point<Pixels>, _window, cx| {
                                this.update(cx, |panel, _| {
                                    let drag = TimelineDrag {
                                        grab_offset: panel.pending_grab,
                                        ..drag.clone()
                                    };
                                    panel.preview = Some(DragPreview {
                                        drag,
                                        time: start_seconds,
                                        track_index: None,
                                        moved: false,
                                    });
                                });
                                cx.new(|_| gpui::Empty)
                            }
                        },
                    ))
                    .when_some(waveform, |this, waveform| {
                        this.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| paint_waveform(bounds, &waveform, window),
                            )
                            .absolute()
                            .inset_0(),
                        )
                    })
                    .when(muted_badge, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(3.))
                                .right(px(HANDLE_WIDTH + 2.0))
                                .px(px(4.))
                                .rounded(px(3.))
                                .bg(gpui::black().opacity(0.55))
                                .text_size(px(9.))
                                .text_color(gpui::white())
                                .child("MUTED"),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px(px(HANDLE_WIDTH + 2.0))
                            .py(px(4.))
                            .text_color(gpui::white())
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .opacity(0.8)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(detail),
                            ),
                    )
                    .children(transition_marks)
                    .children(markers)
                    .when(!locked, |this| this.child(
                        div()
                            .id(eid("trim-start", &id))
                            .absolute()
                            .top_0()
                            .left_0()
                            .w(px(HANDLE_WIDTH))
                            .h_full()
                            .cursor_col_resize()
                            .bg(gpui::white().opacity(if selected { 0.35 } else { 0.0 }))
                            .hover(|style| style.bg(gpui::white().opacity(0.5)))
                            .on_drag(
                                TimelineDrag {
                                    mode: DragMode::TrimStart,
                                    element_id: id.clone(),
                                    grab_offset: 0.0,
                                },
                                |_, _, _, cx| cx.new(|_| gpui::Empty),
                            ),
                    ))
                    .when(!locked, |this| this.child(
                        div()
                            .id(eid("trim-end", &id))
                            .absolute()
                            .top_0()
                            .right_0()
                            .w(px(HANDLE_WIDTH))
                            .h_full()
                            .cursor_col_resize()
                            .bg(gpui::white().opacity(if selected { 0.35 } else { 0.0 }))
                            .hover(|style| style.bg(gpui::white().opacity(0.5)))
                            .on_drag(
                                TimelineDrag {
                                    mode: DragMode::TrimEnd,
                                    element_id: id,
                                    grab_offset: 0.0,
                                },
                                |_, _, _, cx| cx.new(|_| gpui::Empty),
                            ),
                    ))
            });

            let muted = track.muted;
            let hidden = track.hidden;
            let is_audio = track.kind == TrackKind::Audio;
            let is_text = track.kind == TrackKind::Text;
            let toggle_id = track_id.clone();
            let toggle_state = state_entity.clone();
            let header = div()
                .flex()
                .flex_none()
                .w(px(HEADER_WIDTH))
                .h_full()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .border_r_1()
                .border_color(colors.border)
                .bg(colors.card)
                .when(dimmed, |this| this.opacity(0.6))
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
                                .child(track_label),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(colors.muted_foreground)
                                .child(kind_label),
                        ),
                )
                .when(!is_text, |this| {
                    let state = toggle_state.clone();
                    let id = toggle_id.clone();
                    this.child(
                        Button::new(eid("mute", &track_id), if muted { "M" } else { "m" })
                            .variant(if muted { ButtonVariant::Default } else { ButtonVariant::Ghost })
                            .size(ButtonSize::IconXSmall)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    state.execute(
                                        editor::Command::SetTrackMuted {
                                            track_id: id.clone(),
                                            muted: !muted,
                                        },
                                        cx,
                                    );
                                })
                            }),
                    )
                })
                .child({
                    let state = toggle_state.clone();
                    let id = toggle_id.clone();
                    Button::new(eid("lock", &track_id), if locked { "L" } else { "l" })
                        .variant(if locked { ButtonVariant::Default } else { ButtonVariant::Ghost })
                        .size(ButtonSize::IconXSmall)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |state, cx| state.set_track_locked(&id, !locked, cx));
                        })
                })
                .when(!is_audio, |this| {
                    let state = toggle_state.clone();
                    let id = toggle_id.clone();
                    this.child(
                        Button::new(eid("hide", &track_id), if hidden { "H" } else { "h" })
                            .variant(if hidden { ButtonVariant::Default } else { ButtonVariant::Ghost })
                            .size(ButtonSize::IconXSmall)
                            .on_click(move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    state.execute(
                                        editor::Command::SetTrackHidden {
                                            track_id: id.clone(),
                                            hidden: !hidden,
                                        },
                                        cx,
                                    );
                                })
                            }),
                    )
                });

            let focus_for_lane = focus_handle.clone();
            let lane = div()
                .id(eid("lane", &track_id))
                .relative()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .bg(if track.lane == editor::Lane::Main {
                    colors.background
                } else {
                    colors.background.opacity(0.6)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.focus(&focus_for_lane);
                        this.state.update(cx, |state, cx| state.clear_selection(cx));
                        this.scrub_to(event.position.x, cx);
                    }),
                )
                .on_drag(
                    TimelineDrag {
                        mode: DragMode::Scrub,
                        element_id: String::new(),
                        grab_offset: 0.0,
                    },
                    |_, _, _, cx| cx.new(|_| gpui::Empty),
                )
                .children(clips);

            rows.push(
                div()
                    .flex()
                    .h(px(TRACK_HEIGHT))
                    .border_b_1()
                    .border_color(colors.border)
                    .child(header)
                    .child(lane),
            );
        }

        let playhead_x = to_x(playhead.to_seconds_f64());
        if !peaks_wanted.is_empty() {
            peaks_wanted.sort();
            peaks_wanted.dedup();
            self.state.update(cx, |state, _| state.request_peaks(peaks_wanted));
        }
        let playhead_line = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(HEADER_WIDTH + playhead_x - 1.0))
            .w(px(2.))
            .bg(hsla(0.0, 0.85, 0.55, 1.0))
            .when(playhead_x < 0.0, |this| this.invisible());

        let bounds_cell = Rc::clone(&self.rows_bounds);
        let rows_area = div()
            .id("timeline-rows")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .on_drag_move::<TimelineDrag>(cx.listener(Self::on_drag_move))
            .on_drop(cx.listener(|this, _: &TimelineDrag, _, cx| this.finish_drag(cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_drag(cx)),
            )
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(ruler)
            .children(rows)
            .child(
                div()
                    .flex()
                    .h(px(TRACK_HEIGHT))
                    .child(div().flex_none().w(px(HEADER_WIDTH)).border_r_1().border_color(colors.border).bg(colors.card))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .px(px(8.))
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child("Drop media here with Add, or press T for a title. Drag clips to move them, drag their edges to trim."),
                    ),
            )
            .child(playhead_line);

        let tool = |id: &'static str, label: &'static str, enabled: bool, action: fn(&mut EditorState, &mut Context<EditorState>)| {
            let state = state_entity.clone();
            Button::new(id, label)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .disabled(!enabled)
                .on_click(move |_, _, cx| state.update(cx, |state, cx| action(state, cx)))
        };

        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .h(px(34.))
            .px(px(6.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.card)
            .child(tool("undo", "Undo", can_undo, |s, cx| s.undo(cx)))
            .child(tool("redo", "Redo", can_redo, |s, cx| s.redo(cx)))
            .child(div().w(px(8.)))
            .child(tool("split", "Split", true, |s, cx| s.split_at_playhead(cx)))
            .child(tool("duplicate", "Duplicate", !selection.is_empty(), |s, cx| s.duplicate_selection(cx)))
            .child(tool("delete", "Delete", !selection.is_empty(), |s, cx| s.delete_selection(cx)))
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(format!("{}  ·  {} s total", timecode(playhead, rate), short_duration(duration.to_seconds_f64()))),
            )
            .child(div().w(px(8.)))
            .child(tool("zoom-out", "−", true, |s, cx| s.zoom_by(1.0 / 1.4, None, cx)))
            .child(tool("zoom-in", "+", true, |s, cx| s.zoom_by(1.4, None, cx)));

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(toolbar)
            .child(rows_area)
            .when_some(status, |this, status| {
                let state = state_entity.clone();
                this.child(
                    div()
                        .id("status")
                        .flex()
                        .items_center()
                        .h(px(24.))
                        .px(px(8.))
                        .border_t_1()
                        .border_color(colors.border)
                        .bg(colors.card)
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.clear_status(cx)))
                        .child(status),
                )
            })
    }
}


/// What a clip's waveform is drawn from.
struct Waveform {
    peaks: Arc<Vec<f32>>,
    /// Seconds into the source where the clip starts.
    source_start: f64,
    /// Timeline pixels per second.
    zoom: f32,
    /// The clip's volume in dB when it has no keys.
    static_db: f64,
    volume: Option<editor::Channel>,
    muted: bool,
}

impl Waveform {
    /// Linear gain at a time into the clip, from the volume keys or the
    /// static volume.
    fn gain_at(&self, local_seconds: f64) -> f32 {
        let db = self
            .volume
            .as_ref()
            .filter(|channel| !channel.is_empty())
            .and_then(|channel| channel.value_at(MediaTime::from_seconds_f64(local_seconds).unwrap_or(MediaTime::ZERO)))
            .unwrap_or(self.static_db);
        (10f64.powf(db / 20.0) as f32).clamp(0.0, 2.0)
    }
}

/// Peaks as a centred bar per pixel column, scaled by the gain at that
/// point, with the gain curve drawn over them (mid-height is 0 dB).
/// The start (or the end, for the clip's end) nearest a marker within
/// `tolerance` seconds, else `start` unchanged.
fn snap_to_markers(start: f64, duration: f64, markers: &[time::MediaTime], tolerance: f64) -> f64 {
    let mut best: Option<(f64, f64)> = None;
    for marker in markers {
        let marker = marker.to_seconds_f64();
        for candidate in [marker, marker - duration] {
            if candidate < 0.0 {
                continue;
            }
            let distance = (candidate - start).abs();
            if distance <= tolerance && best.is_none_or(|(_, d)| distance < d) {
                best = Some((candidate, distance));
            }
        }
    }
    best.map(|(candidate, _)| candidate).unwrap_or(start)
}

fn paint_waveform(bounds: Bounds<Pixels>, waveform: &Waveform, window: &mut Window) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width < 1.0 || height < 4.0 || waveform.peaks.is_empty() {
        return;
    }
    let peaks_per_second = crate::render_thread::PEAKS_PER_SECOND as f64;
    let middle = height / 2.0;
    let bar = gpui::white().opacity(if waveform.muted { 0.18 } else { 0.45 });
    let line = if waveform.muted { gpui::white().opacity(0.25) } else { hsla(0.14, 0.9, 0.65, 0.9) };
    let columns = width.ceil() as usize;
    let mut previous_y: Option<f32> = None;
    for column in 0..columns {
        let x = column as f32;
        let local_seconds = x as f64 / waveform.zoom.max(0.01) as f64;
        let source_seconds = waveform.source_start + local_seconds;
        // The loudest peak in the stretch of source this column covers.
        let first = (source_seconds * peaks_per_second).floor().max(0.0) as usize;
        let last = ((source_seconds + 1.0 / waveform.zoom.max(0.01) as f64) * peaks_per_second).ceil() as usize;
        let peak = waveform.peaks[first.min(waveform.peaks.len())..last.min(waveform.peaks.len())]
            .iter()
            .fold(0.0f32, |peak, value| peak.max(*value));
        let gain = waveform.gain_at(local_seconds);
        let half = (peak * gain.min(1.0) * (middle - 2.0)).max(0.5);
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x + px(x), bounds.origin.y + px(middle - half)),
                size(px(1.0), px(half * 2.0)),
            ),
            bar,
        ));
        // Gain line: 0 dB at the middle, +6 dB at the top, silence at the bottom.
        let y = height - (gain / 2.0).min(1.0) * (height - 2.0) - 1.0;
        if column % 2 == 0 {
            let top = previous_y.map(|p| p.min(y)).unwrap_or(y);
            let bottom = previous_y.map(|p| p.max(y)).unwrap_or(y);
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + px(x), bounds.origin.y + px(top - 0.75)),
                    size(px(2.0), px((bottom - top) + 1.5)),
                ),
                line,
            ));
            previous_y = Some(y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_line_follows_static_and_keyed_volume() {
        let flat = Waveform { peaks: Arc::new(vec![1.0]), source_start: 0.0, zoom: 100.0, static_db: -6.0, volume: None, muted: false };
        assert!((flat.gain_at(0.5) - 0.501).abs() < 0.01);
        let mut channel = editor::Channel::default();
        channel.upsert(MediaTime::ZERO, 0.0, None);
        channel.upsert(MediaTime::from_seconds_f64(1.0).unwrap(), -20.0, None);
        let keyed = Waveform { volume: Some(channel), ..flat };
        assert!((keyed.gain_at(0.0) - 1.0).abs() < 1e-6);
        assert!((keyed.gain_at(1.0) - 0.1).abs() < 1e-6);
        assert!(keyed.gain_at(0.5) < 1.0 && keyed.gain_at(0.5) > 0.1);
    }
}

#[cfg(test)]
mod marker_tests {
    use super::snap_to_markers;
    use time::MediaTime;

    #[test]
    fn snap_to_markers_prefers_the_nearest_edge() {
        let markers: Vec<MediaTime> = [2.0, 5.0].iter().map(|s| MediaTime::from_seconds_f64(*s).unwrap()).collect();
        // The start lands on the 2 s marker; the end (start + 1 s) would land on 5 s from 4.03.
        assert_eq!(snap_to_markers(1.95, 1.0, &markers, 0.1), 2.0);
        assert_eq!(snap_to_markers(4.03, 1.0, &markers, 0.1), 4.0);
        assert_eq!(snap_to_markers(3.0, 1.0, &markers, 0.1), 3.0, "nothing within reach stays put");
        assert_eq!(snap_to_markers(0.0, 3.0, &markers, 0.1), 0.0, "a negative candidate is skipped");
    }
}

/// A marker's `#rrggbb` colour (the default blue when missing or unreadable).
fn marker_color(color: Option<&str>) -> gpui::Hsla {
    let hex = color.and_then(|text| text.strip_prefix('#')).filter(|text| text.len() == 6);
    let value = hex
        .and_then(|text| u32::from_str_radix(text, 16).ok())
        .unwrap_or(0x009dff);
    gpui::rgb(value).into()
}

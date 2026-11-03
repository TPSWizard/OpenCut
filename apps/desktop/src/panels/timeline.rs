use std::cell::Cell;
use std::rc::Rc;

use editor::{Edge, Element, ElementKind, TrackKind};
use gpui::{
    Bounds, Context, DragMoveEvent, Entity, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, Window, canvas, div, hsla, prelude::*, px,
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
                let start = (pointer_seconds - drag.grab_offset).max(0.0);
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
        let can_undo = state.editor.can_undo();
        let can_redo = state.editor.can_redo();
        let selection = state.selection.clone();
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
                    .children(ticks),
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

                // Keyframe markers: one dot per distinct key time, on the
                // selected clip only.
                let mut key_times: Vec<MediaTime> = if selected {
                    element
                        .animations
                        .values()
                        .flat_map(|channel| channel.keys.iter().map(|key| key.time))
                        .filter(|&time| time >= MediaTime::ZERO && time <= element.duration)
                        .collect()
                } else {
                    Vec::new()
                };
                key_times.sort();
                key_times.dedup();
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
                    .overflow_hidden()
                    .cursor_grab()
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
                    .on_drag(
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
                    )
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
                    .children(markers)
                    .child(
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
                    )
                    .child(
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
                    )
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

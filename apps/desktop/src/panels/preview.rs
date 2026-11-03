use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Bounds, Context, Entity, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit, Path, Pixels, Point, Window,
    canvas, div, fill, hsla, img, point, prelude::*, px, size,
};

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::format::timecode;
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// The composited frame at the playhead, with transport controls.
pub(crate) struct PreviewPanel {
    state: Entity<EditorState>,
    /// Size of the frame area from the last layout, so the renderer can
    /// produce exactly the pixels that are shown.
    frame_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Canvas pixel where the pen's last point was pressed, until the
    /// button comes up: dragging from it pulls out the point's handles.
    pen_press: Rc<Cell<Option<(f32, f32)>>>,
}

impl PreviewPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            frame_bounds: Rc::new(Cell::new(Bounds::default())),
            pen_press: Rc::new(Cell::new(None)),
        }
    }
}

/// Where the frame (object-fit contain) sits inside the frame area:
/// origin and size on screen.
fn fitted_frame(bounds: Bounds<Pixels>, (canvas_width, canvas_height): (f32, f32)) -> (Point<Pixels>, f32) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let scale = (width / canvas_width.max(1.0)).min(height / canvas_height.max(1.0)).max(0.0);
    let origin = point(
        bounds.origin.x + px((width - canvas_width * scale) / 2.0),
        bounds.origin.y + px((height - canvas_height * scale) / 2.0),
    );
    (origin, scale)
}

/// A screen position as canvas pixels, when it is on the frame.
fn canvas_point(bounds: Bounds<Pixels>, canvas_size: (f32, f32), position: Point<Pixels>) -> Option<(f32, f32)> {
    let (origin, scale) = fitted_frame(bounds, canvas_size);
    if scale <= 0.0 {
        return None;
    }
    let x = f32::from(position.x - origin.x) / scale;
    let y = f32::from(position.y - origin.y) / scale;
    (x >= 0.0 && y >= 0.0 && x <= canvas_size.0 && y <= canvas_size.1).then_some((x, y))
}

/// Draws a freeform path's points and segments over the frame.
fn paint_path_overlay(
    bounds: Bounds<Pixels>,
    canvas_size: (f32, f32),
    points: &[(f64, f64)],
    closed: bool,
    window: &mut Window,
) {
    let (origin, scale) = fitted_frame(bounds, canvas_size);
    if scale <= 0.0 || points.is_empty() {
        return;
    }
    let on_screen: Vec<Point<Pixels>> = points
        .iter()
        .map(|&(x, y)| point(origin.x + px(x as f32 * scale), origin.y + px(y as f32 * scale)))
        .collect();
    let line = hsla(0.55, 0.9, 0.6, 0.95);
    let segments = if closed { on_screen.len() } else { on_screen.len().saturating_sub(1) };
    for index in 0..segments {
        let a = on_screen[index];
        let b = on_screen[(index + 1) % on_screen.len()];
        let (dx, dy) = (f32::from(b.x - a.x), f32::from(b.y - a.y));
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            continue;
        }
        // A one-pixel-wide quad along the segment.
        let (nx, ny) = (-dy / length * 0.75, dx / length * 0.75);
        let mut path = Path::new(point(a.x + px(nx), a.y + px(ny)));
        path.line_to(point(b.x + px(nx), b.y + px(ny)));
        path.line_to(point(b.x - px(nx), b.y - px(ny)));
        path.line_to(point(a.x - px(nx), a.y - px(ny)));
        window.paint_path(path, line);
    }
    for (index, &center) in on_screen.iter().enumerate() {
        let half = px(if index == 0 { 4.0 } else { 3.0 });
        let color = if index == 0 { hsla(0.1, 0.95, 0.6, 1.0) } else { hsla(0.0, 0.0, 1.0, 1.0) };
        window.paint_quad(fill(
            Bounds::new(point(center.x - half, center.y - half), size(half * 2.0, half * 2.0)),
            color,
        ));
    }
}

impl Render for PreviewPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;

        // Tell the renderer how big the frame is on screen (device pixels).
        let bounds = self.frame_bounds.get();
        let scale = window.scale_factor();
        let shown_width = (f32::from(bounds.size.width) * scale).round() as u32;
        let shown_height = (f32::from(bounds.size.height) * scale).round() as u32;
        if shown_width > 0 && shown_height > 0 {
            self.state.update(cx, |state, cx| {
                state.set_preview_size(shown_width, shown_height, cx);
            });
        }

        let state = self.state.read(cx);
        let project = state.project();
        let rate = project.settings.fps;
        let playhead = timecode(state.playhead(), rate);
        let duration = timecode(state.duration(), rate);
        let playing = state.playing;
        let preview = state.preview.clone();
        let focus_handle = state.focus_handle.clone();
        let canvas_label = format!(
            "{}×{} · {} fps",
            project.settings.canvas.width,
            project.settings.canvas.height,
            rate.as_f64().map(|f| format!("{f:.2}")).unwrap_or_default()
        );

        let canvas_size = (project.settings.canvas.width as f32, project.settings.canvas.height as f32);
        let pen_active = state.pen.is_some();
        let overlay = state.freeform_overlay();

        let bounds_cell = Rc::clone(&self.frame_bounds);
        let frame_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .bg(gpui::black())
            .when(pen_active, |this| this.cursor(gpui::CursorStyle::Crosshair))
            .on_mouse_down(MouseButton::Left, {
                let focus_handle = focus_handle.clone();
                let state = self.state.clone();
                let bounds_cell = Rc::clone(&self.frame_bounds);
                let pen_press = Rc::clone(&self.pen_press);
                move |event: &MouseDownEvent, window, cx| {
                    window.focus(&focus_handle);
                    if !pen_active {
                        return;
                    }
                    let Some((x, y)) = canvas_point(bounds_cell.get(), canvas_size, event.position) else {
                        return;
                    };
                    pen_press.set(None);
                    state.update(cx, |state, cx| {
                        state.pen_add_point(x as f64, y as f64, cx);
                        if event.click_count >= 2 {
                            state.finish_pen(cx);
                        } else {
                            pen_press.set(Some((x, y)));
                        }
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let state = self.state.clone();
                let bounds_cell = Rc::clone(&self.frame_bounds);
                let pen_press = Rc::clone(&self.pen_press);
                move |event: &MouseUpEvent, _, cx| {
                    let Some((x0, y0)) = pen_press.take() else {
                        return;
                    };
                    let Some((x, y)) = canvas_point(bounds_cell.get(), canvas_size, event.position) else {
                        return;
                    };
                    // A short drag is a click; a longer one bends the path.
                    if (x - x0).powi(2) + (y - y0).powi(2) < 16.0 {
                        return;
                    }
                    state.update(cx, |state, cx| {
                        state.pen_drag_handles((x0 as f64, y0 as f64), (x as f64, y as f64), cx)
                    });
                }
            })
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .map(|this| match preview {
                Some(image) => this.child(img(image).size_full().object_fit(ObjectFit::Contain)),
                None => this.child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Rendering…"),
                ),
            })
            .when_some(overlay, |this, (points, closed)| {
                this.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| paint_path_overlay(bounds, canvas_size, &points, closed, window),
                    )
                    .absolute()
                    .inset_0(),
                )
            });

        let transport = |id: &'static str, label: &'static str, action: fn(&mut EditorState, &mut Context<EditorState>)| {
            let state = self.state.clone();
            let focus_handle = focus_handle.clone();
            Button::new(id, label)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .on_click(move |_, window, cx| {
                    window.focus(&focus_handle);
                    state.update(cx, |state, cx| action(state, cx));
                })
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .child(frame_area)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(36.))
                    .px(px(8.))
                    .border_t_1()
                    .border_color(colors.border)
                    .child(transport("go-start", "|<", |state, cx| {
                        state.seek(time::MediaTime::ZERO, cx)
                    }))
                    .child(transport("step-back", "<", |state, cx| state.step_frames(-1, cx)))
                    .child({
                        let state = self.state.clone();
                        let focus_handle = focus_handle.clone();
                        Button::new("play", if playing { "Pause" } else { "Play" })
                            .size(ButtonSize::Small)
                            .on_click(move |_, window, cx| {
                                window.focus(&focus_handle);
                                state.update(cx, |state, cx| state.toggle_playback(cx));
                            })
                    })
                    .child(transport("step-forward", ">", |state, cx| state.step_frames(1, cx)))
                    .child(transport("go-end", ">|", |state, cx| {
                        let end = state.duration();
                        state.seek(end, cx)
                    }))
                    .child(
                        div()
                            .ml(px(8.))
                            .text_xs()
                            .text_color(colors.foreground)
                            .child(format!("{playhead}  /  {duration}")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child(canvas_label),
                    ),
            )
    }
}

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use editor::Params;
use gpui::{
    Bounds, Context, DispatchPhase, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    Path, Pixels, Point, Window, canvas, div, fill, hsla, img, point, prelude::*, px, size,
};

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::format::timecode;
use crate::state::{EditorState, FreeformHit, FreeformOverlay};
use crate::theme::ActiveTheme;
use crate::i18n::{t, tf};

/// How far (in screen pixels) a click may be from a path point or segment
/// to take hold of it, and how far a press must travel to count as a drag.
const HIT_PX: f32 = 7.0;
const DRAG_PX: f32 = 4.0;

/// A press on the selected clip's path, until the button comes up: an
/// anchor being moved, or a segment that inserts a point (click) or moves
/// the whole path (drag).
struct PathDrag {
    hit: FreeformHit,
    /// Canvas pixel of the press and of the latest move.
    start: (f32, f32),
    last: (f32, f32),
    /// The mask's parameters at the press; every move is computed from them.
    start_params: Params,
    moved: bool,
}

/// The composited frame at the playhead, with transport controls.
pub(crate) struct PreviewPanel {
    state: Entity<EditorState>,
    /// Size of the frame area from the last layout, so the renderer can
    /// produce exactly the pixels that are shown.
    frame_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Canvas pixel where the pen's last point was pressed, until the
    /// button comes up: dragging from it pulls out the point's handles.
    pen_press: Rc<Cell<Option<(f32, f32)>>>,
    /// A path point or segment being dragged on the preview.
    path_drag: Rc<RefCell<Option<PathDrag>>>,
}

impl PreviewPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            frame_bounds: Rc::new(Cell::new(Bounds::default())),
            pen_press: Rc::new(Cell::new(None)),
            path_drag: Rc::new(RefCell::new(None)),
        }
    }
}

/// Follows the pointer during a path drag: the anchor or the whole path
/// is previewed at the pointer, once it has travelled far enough to be a
/// drag rather than a click.
fn track_path_drag(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    canvas_size: (f32, f32),
    path_drag: &Rc<RefCell<Option<PathDrag>>>,
    state: &Entity<EditorState>,
    cx: &mut gpui::App,
) {
    let (origin, scale) = fitted_frame(bounds, canvas_size);
    if scale <= 0.0 {
        return;
    }
    let x = f32::from(position.x - origin.x) / scale;
    let y = f32::from(position.y - origin.y) / scale;
    let step = {
        let mut borrowed = path_drag.borrow_mut();
        let Some(drag) = borrowed.as_mut() else {
            return;
        };
        drag.last = (x, y);
        let travelled = ((x - drag.start.0).powi(2) + (y - drag.start.1).powi(2)).sqrt() * scale;
        if !drag.moved && travelled < DRAG_PX {
            return;
        }
        drag.moved = true;
        (drag.hit.clone(), drag.start, drag.start_params.clone())
    };
    let (hit, start, start_params) = step;
    state.update(cx, |state, cx| match hit {
        FreeformHit::Anchor(id) => state.drag_mask_point(&start_params, &id, (x as f64, y as f64), false, cx),
        FreeformHit::Segment(..) => {
            let delta = ((x - start.0) as f64, (y - start.1) as f64);
            state.drag_mask_path(&start_params, delta, false, cx)
        }
    });
}

/// Ends a path drag: a moved anchor or path is committed as one edit, a
/// click on a segment inserts a point there.
fn finish_path_drag(drag: PathDrag, state: &Entity<EditorState>, cx: &mut gpui::App) {
    let PathDrag { hit, start, last, start_params, moved } = drag;
    state.update(cx, |state, cx| match hit {
        FreeformHit::Anchor(id) => {
            if moved {
                state.drag_mask_point(&start_params, &id, (last.0 as f64, last.1 as f64), true, cx);
            }
        }
        FreeformHit::Segment(segment, t) => {
            if moved {
                let delta = ((last.0 - start.0) as f64, (last.1 - start.1) as f64);
                state.drag_mask_path(&start_params, delta, true, cx);
            } else {
                state.insert_mask_point(segment, t, cx);
            }
        }
    });
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

/// Shades the margins a platform's own controls cover and outlines what
/// stays visible, as the web app's guide overlay does.
fn paint_safe_area(bounds: Bounds<Pixels>, canvas_size: (f32, f32), area: editor::SafeArea, window: &mut Window) {
    let (origin, scale) = fitted_frame(bounds, canvas_size);
    if scale <= 0.0 {
        return;
    }
    let (width, height) = (canvas_size.0 * scale, canvas_size.1 * scale);
    let top = height * area.top as f32;
    let bottom = height * area.bottom as f32;
    let left = width * area.left as f32;
    let right = width * area.right as f32;
    let shade = hsla(0.0, 0.0, 0.0, 0.45);
    let quads = [
        (0.0, 0.0, width, top),
        (0.0, height - bottom, width, bottom),
        (0.0, top, left, height - top - bottom),
        (width - right, top, right, height - top - bottom),
    ];
    for (x, y, w, h) in quads {
        if w > 0.0 && h > 0.0 {
            window.paint_quad(fill(
                Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h))),
                shade,
            ));
        }
    }
    let line = hsla(0.0, 0.0, 1.0, 0.9);
    let corners = [
        point(origin.x + px(left), origin.y + px(top)),
        point(origin.x + px(width - right), origin.y + px(top)),
        point(origin.x + px(width - right), origin.y + px(height - bottom)),
        point(origin.x + px(left), origin.y + px(height - bottom)),
    ];
    for index in 0..4 {
        paint_line(corners[index], corners[(index + 1) % 4], 0.75, line, window);
    }
}

/// A thin quad from `a` to `b`.
fn paint_line(a: Point<Pixels>, b: Point<Pixels>, half_width: f32, color: gpui::Hsla, window: &mut Window) {
    let (dx, dy) = (f32::from(b.x - a.x), f32::from(b.y - a.y));
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 {
        return;
    }
    let (nx, ny) = (-dy / length * half_width, dx / length * half_width);
    let mut path = Path::new(point(a.x + px(nx), a.y + px(ny)));
    path.line_to(point(b.x + px(nx), b.y + px(ny)));
    path.line_to(point(b.x - px(nx), b.y - px(ny)));
    path.line_to(point(a.x - px(nx), a.y - px(ny)));
    window.paint_path(path, color);
}

fn paint_square(center: Point<Pixels>, half: f32, color: gpui::Hsla, window: &mut Window) {
    let half = px(half);
    window.paint_quad(fill(
        Bounds::new(point(center.x - half, center.y - half), size(half * 2.0, half * 2.0)),
        color,
    ));
}

/// Draws a freeform path over the frame: its curved segments, its anchors
/// (the first one orange, selected ones blue) and the handles of selected
/// anchors.
fn paint_path_overlay(bounds: Bounds<Pixels>, canvas_size: (f32, f32), overlay: &FreeformOverlay, window: &mut Window) {
    let (origin, scale) = fitted_frame(bounds, canvas_size);
    if scale <= 0.0 || overlay.points.is_empty() {
        return;
    }
    let to_screen = |(x, y): (f64, f64)| point(origin.x + px(x as f32 * scale), origin.y + px(y as f32 * scale));
    let line = hsla(0.55, 0.9, 0.6, 0.95);
    let count = overlay.points.len();
    let segments = if overlay.closed { count } else { count.saturating_sub(1) };
    for index in 0..segments {
        let a = &overlay.points[index];
        let b = &overlay.points[(index + 1) % count];
        let straight = a.out_handle == a.anchor && b.in_handle == b.anchor;
        if straight {
            paint_line(to_screen(a.anchor), to_screen(b.anchor), 0.75, line, window);
            continue;
        }
        let mut previous = to_screen(a.anchor);
        for step in 1..=16 {
            let t = step as f64 / 16.0;
            let next = to_screen(editor::cubic_point(a.anchor, a.out_handle, b.in_handle, b.anchor, t));
            paint_line(previous, next, 0.75, line, window);
            previous = next;
        }
    }
    let handle_color = hsla(0.55, 0.9, 0.75, 0.9);
    for (index, point) in overlay.points.iter().enumerate() {
        let anchor = to_screen(point.anchor);
        if point.selected {
            for handle in [point.in_handle, point.out_handle] {
                if handle != point.anchor {
                    let end = to_screen(handle);
                    paint_line(anchor, end, 0.5, handle_color, window);
                    paint_square(end, 2.5, handle_color, window);
                }
            }
        }
        let (half, color) = match (point.selected, index == 0) {
            (true, _) => (4.5, hsla(0.55, 0.95, 0.55, 1.0)),
            (false, true) => (4.0, hsla(0.1, 0.95, 0.6, 1.0)),
            (false, false) => (3.0, hsla(0.0, 0.0, 1.0, 1.0)),
        };
        paint_square(anchor, half, color, window);
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
                state.ensure_stickers(cx);
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
        let guide = state.guide;
        let pen_active = state.pen.is_some();
        let overlay = state.freeform_overlay();
        let path_editable = overlay.as_ref().is_some_and(|overlay| overlay.editable);
        let dragging = self.path_drag.borrow().is_some();

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
            .when(dragging, |this| this.cursor(gpui::CursorStyle::ClosedHand))
            .on_mouse_down(MouseButton::Left, {
                let focus_handle = focus_handle.clone();
                let state = self.state.clone();
                let bounds_cell = Rc::clone(&self.frame_bounds);
                let pen_press = Rc::clone(&self.pen_press);
                let path_drag = Rc::clone(&self.path_drag);
                move |event: &MouseDownEvent, window, cx| {
                    window.focus(&focus_handle);
                    let bounds = bounds_cell.get();
                    let Some((x, y)) = canvas_point(bounds, canvas_size, event.position) else {
                        return;
                    };
                    if pen_active {
                        pen_press.set(None);
                        state.update(cx, |state, cx| {
                            // The first click of a double-click already placed
                            // the last point; the second only closes the path.
                            if event.click_count >= 2 {
                                state.finish_pen(cx);
                            } else {
                                state.pen_add_point(x as f64, y as f64, cx);
                                pen_press.set(Some((x, y)));
                            }
                        });
                        return;
                    }
                    if !path_editable {
                        return;
                    }
                    // A closed path's anchors move, its segments take a new
                    // point (click) or move the path (drag), as in the web app.
                    let (_, scale) = fitted_frame(bounds, canvas_size);
                    let tolerance = (HIT_PX / scale.max(0.01)) as f64;
                    let hit = state.read(cx).freeform_hit((x as f64, y as f64), tolerance);
                    let start_params = state.read(cx).freeform_mask().map(|(_, mask)| mask.params.clone());
                    match (hit, start_params) {
                        (Some(hit), Some(start_params)) => {
                            if let FreeformHit::Anchor(id) = &hit {
                                let id = id.clone();
                                let extend = event.modifiers.shift;
                                state.update(cx, |state, cx| state.select_mask_point(&id, extend, cx));
                                if extend {
                                    return;
                                }
                            }
                            *path_drag.borrow_mut() = Some(PathDrag {
                                hit,
                                start: (x, y),
                                last: (x, y),
                                start_params,
                                moved: false,
                            });
                            // Redraw so the window-level tracking below is registered.
                            window.refresh();
                        }
                        _ => state.update(cx, |state, cx| state.clear_mask_point_selection(cx)),
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let state = self.state.clone();
                let bounds_cell = Rc::clone(&self.frame_bounds);
                let pen_press = Rc::clone(&self.pen_press);
                let path_drag = Rc::clone(&self.path_drag);
                move |event: &MouseUpEvent, _, cx| {
                    let drag = path_drag.borrow_mut().take();
                    if let Some(drag) = drag {
                        finish_path_drag(drag, &state, cx);
                        return;
                    }
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
            .on_mouse_up_out(MouseButton::Left, {
                let state = self.state.clone();
                let pen_press = Rc::clone(&self.pen_press);
                let path_drag = Rc::clone(&self.path_drag);
                move |_: &MouseUpEvent, _, cx| {
                    pen_press.set(None);
                    let drag = path_drag.borrow_mut().take();
                    if let Some(drag) = drag {
                        finish_path_drag(drag, &state, cx);
                    }
                }
            })
            .child({
                // While a path point or segment is dragged, the pointer is
                // followed anywhere in the window (element listeners only
                // fire while hovered), as the web app's pointer capture does.
                let state = self.state.clone();
                let path_drag = Rc::clone(&self.path_drag);
                let tracked_bounds = Rc::clone(&self.frame_bounds);
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    move |_, _, window, _| {
                        if dragging {
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    track_path_drag(event.position, tracked_bounds.get(), canvas_size, &path_drag, &state, cx);
                                }
                            });
                        }
                    },
                )
                .absolute()
                .inset_0()
            })
            .map(|this| match preview {
                Some(image) => this.child(img(image).size_full().object_fit(ObjectFit::Contain)),
                None => this.child(
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(t("Rendering…")),
                ),
            })
            .when_some(guide.and_then(|guide| guide.safe_area), |this, safe_area| {
                this.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| paint_safe_area(bounds, canvas_size, safe_area, window),
                    )
                    .absolute()
                    .inset_0(),
                )
            })
            .when_some(overlay, |this, overlay| {
                this.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| paint_path_overlay(bounds, canvas_size, &overlay, window),
                    )
                    .absolute()
                    .inset_0(),
                )
            });

        // Platform safe-area guides: the presets that publish one, plus "Off".
        let mut guide_row = div().flex().items_center().gap(px(2.)).ml(px(8.)).child(
            div().text_xs().text_color(colors.muted_foreground).mr(px(4.)).child(t("Guide")),
        );
        let guide_choices = std::iter::once(None)
            .chain(editor::PLATFORM_PRESETS.iter().filter(|preset| preset.safe_area.is_some()).map(Some));
        for choice in guide_choices {
            let state = self.state.clone();
            let label = choice.map(|preset| preset.label).unwrap_or("Off");
            let shown = if choice.is_some() { label } else { t("Off") };
            let active = match (choice, guide) {
                (None, None) => true,
                (Some(a), Some(b)) => a.id == b.id,
                _ => false,
            };
            guide_row = guide_row.child(
                Button::new(crate::components::eid("guide", label), shown)
                    .variant(if active { ButtonVariant::Default } else { ButtonVariant::Ghost })
                    .size(ButtonSize::XSmall)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| state.set_guide(choice, cx));
                    }),
            );
        }

        // Preview quality: how many pixels are rendered, and proxies on/off.
        let quality = state.settings.preview_quality;
        let use_proxies = state.settings.use_proxies;
        let (pixels_w, pixels_h) = state.preview_pixels();
        let mut quality_row = div().flex().items_center().gap(px(2.)).ml(px(8.)).child(
            div().text_xs().text_color(colors.muted_foreground).mr(px(4.)).child(t("Quality")),
        );
        for choice in crate::settings::PreviewQuality::ALL {
            let state = self.state.clone();
            quality_row = quality_row.child(
                Button::new(crate::components::eid("quality", choice.label()), t(choice.label()))
                    .variant(if choice == quality { ButtonVariant::Default } else { ButtonVariant::Ghost })
                    .size(ButtonSize::XSmall)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| state.set_preview_quality(choice, cx));
                    }),
            );
        }
        quality_row = quality_row.child({
            let state = self.state.clone();
            Button::new("use-proxies", t("Proxies"))
                .variant(if use_proxies { ButtonVariant::Default } else { ButtonVariant::Ghost })
                .size(ButtonSize::XSmall)
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| state.set_use_proxies(!use_proxies, cx));
                })
        });
        let canvas_label = tf("{canvas} · preview {width}×{height}", &[("canvas", &canvas_label), ("width", &pixels_w), ("height", &pixels_h)]);

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
                        Button::new("play", if playing { t("Pause") } else { t("Play") })
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
                    .child(guide_row)
                    .child(quality_row)
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

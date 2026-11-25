//! Inspector controls for colour: the HSL effect's band swatches and hue
//! curves, and colour wheels over lift / gamma / gain. The maths is the
//! editor crate's (`editor::hsl_band_value`, `editor::colour_wheel_*`),
//! shared with the web app's panels.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use editor::Params;
use gpui::{
    Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Path, Pixels, Point, RenderImage, Window, canvas,
    div, fill, img, point, prelude::*, px, size,
};

use super::InspectorPanel;
use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::i18n::t;
use crate::theme::ActiveTheme;

/// Side of a colour wheel (logical pixels).
const WHEEL_SIZE: f32 = 84.0;

/// A colour wheel being dragged: its dot follows the pointer and the
/// offsets are written once, on release (one undo step).
pub(super) struct WheelDrag {
    key: String,
    effect_id: String,
    wheel: usize,
    base: [f64; 3],
    point: (f64, f64),
}

/// Bounds of each wheel from the last layout, by key.
pub(super) type WheelBounds = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

fn paint_segment(a: Point<Pixels>, b: Point<Pixels>, half_width: f32, color: gpui::Hsla, window: &mut Window) {
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

fn colour([r, g, b]: [u8; 3]) -> gpui::Hsla {
    gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

impl InspectorPanel {
    /// The HSL effect's band picker and its three hue curves (hue,
    /// saturation, lightness across the hue circle). Only the picked
    /// band's rows are shown under it.
    pub(super) fn hsl_header(&self, effect_id: &str, params: &Params, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let colors = window.theme().colors;
        let mut swatches = div().flex().flex_wrap().items_center().gap(px(4.)).child(Self::label_cell(t("Colour"), window));
        for (band, (key, _, _)) in editor::HSL_BANDS.iter().enumerate() {
            let selected = band == self.hsl_band;
            swatches = swatches.child(
                div()
                    .id(eid(&format!("hsl-band-{effect_id}"), key))
                    .size(px(18.))
                    .rounded_full()
                    .bg(colour(editor::hsl_band_swatch(band)))
                    .border_2()
                    .border_color(if selected { colors.foreground } else { gpui::transparent_black() })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.hsl_band = band;
                        cx.notify();
                    })),
            );
        }
        swatches = swatches.child(div().text_xs().text_color(colors.foreground).child(t(editor::HSL_BANDS[self.hsl_band].1)));
        let curves: Vec<[f64; 8]> = (0..3)
            .map(|control| std::array::from_fn(|band| params.number(&editor::hsl_param_key(band, control), 0.0)))
            .collect();
        let picked = editor::HSL_BANDS[self.hsl_band].2;
        let line = colors.muted_foreground;
        let curve_view = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let (left, top) = (bounds.origin.x, bounds.origin.y);
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height) - 8.0;
                // The hue strip along the bottom.
                let steps = 72;
                for step in 0..steps {
                    let hue = step as f64 * 360.0 / steps as f64;
                    let x0 = left + px(width * step as f32 / steps as f32);
                    let x1 = left + px(width * (step + 1) as f32 / steps as f32);
                    let rgb = hue_colour(hue);
                    window.paint_quad(fill(Bounds::new(point(x0, top + px(height + 2.0)), size(x1 - x0, px(6.0))), colour(rgb)));
                }
                // Zero line and the picked band's centre.
                paint_segment(point(left, top + px(height / 2.0)), point(left + px(width), top + px(height / 2.0)), 0.5, line.opacity(0.5), window);
                let picked_x = left + px(width * (picked / 360.0) as f32);
                paint_segment(point(picked_x, top), point(picked_x, top + px(height)), 0.5, line.opacity(0.6), window);
                let tones: [gpui::Hsla; 3] = [colour([235, 235, 235]), colour([250, 200, 60]), colour([140, 170, 255])];
                for (values, tone) in curves.iter().zip(tones) {
                    let mut last: Option<Point<Pixels>> = None;
                    for step in 0..=steps {
                        let hue = step as f64 * 360.0 / steps as f64;
                        let value = editor::hsl_band_value(values, hue).clamp(-100.0, 100.0);
                        let at = point(left + px(width * step as f32 / steps as f32), top + px(height * (1.0 - (value as f32 + 100.0) / 200.0)));
                        if let Some(previous) = last {
                            paint_segment(previous, at, 0.75, tone, window);
                        }
                        last = Some(at);
                    }
                }
            },
        )
        .w_full()
        .h(px(64.));
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(swatches)
            .child(curve_view)
            .child(div().text_xs().text_color(colors.muted_foreground).child(t("Curves across the hues: hue (white), saturation (yellow), lightness (blue).")))
    }

    /// Colour wheels for lift, gamma and gain: drag a dot to tint that
    /// range; Centre takes the tint off (the brightness part stays).
    pub(super) fn colour_wheels(&mut self, effect_id: &str, params: &Params, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let colors = window.theme().colors;
        let image = self
            .wheel_image
            .get_or_insert_with(|| {
                let side = (WHEEL_SIZE * 2.0) as usize;
                let mut bgra = editor::colour_wheel_rgba(side);
                for pixel in bgra.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                crate::images::image_from_bgra(side as u32, side as u32, bgra).unwrap_or_else(|| Arc::new(RenderImage::new(smallvec::smallvec![])))
            })
            .clone();
        let mut row = div().flex().flex_wrap().gap(px(10.));
        for (wheel, (label, keys)) in editor::COLOUR_WHEELS.iter().enumerate() {
            let key = format!("{effect_id}:{wheel}");
            let offsets = keys.map(|key| params.number(key, 0.0));
            let (x, y) = match &self.wheel_drag {
                Some(drag) if drag.key == key => drag.point,
                _ => editor::colour_wheel_point(offsets),
            };
            let magnitude = x.hypot(y).max(1.0);
            let (x, y) = (x / magnitude, y / magnitude);
            let half = WHEEL_SIZE / 2.0;
            let (dot_x, dot_y) = (half + x as f32 * half, half - y as f32 * half);
            let bounds = Rc::clone(&self.wheel_bounds);
            let bounds_key = key.clone();
            let wheel_disc = div()
                .id(eid("colour-wheel", &key))
                .relative()
                .size(px(WHEEL_SIZE))
                .flex_none()
                .cursor_pointer()
                .child(img(image.clone()).size_full())
                .child(
                    canvas(move |b, _, _| {
                        bounds.borrow_mut().insert(bounds_key.clone(), b);
                    }, |_, _, _, _| ())
                    .absolute()
                    .inset_0(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(dot_x - 5.0))
                        .top(px(dot_y - 5.0))
                        .size(px(10.))
                        .rounded_full()
                        .border_2()
                        .border_color(gpui::white())
                        .bg(gpui::black().opacity(0.4)),
                )
                .on_mouse_down(MouseButton::Left, {
                    let (key, effect_id) = (key.clone(), effect_id.to_owned());
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        let point = this.wheel_point_at(&key, event.position);
                        this.wheel_drag = Some(WheelDrag { key: key.clone(), effect_id: effect_id.clone(), wheel, base: offsets, point });
                        cx.notify();
                    })
                })
                .on_mouse_move({
                    let key = key.clone();
                    cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            return;
                        }
                        let point = this.wheel_point_at(&key, event.position);
                        if let Some(drag) = this.wheel_drag.as_mut().filter(|drag| drag.key == key) {
                            drag.point = point;
                            cx.notify();
                        }
                    })
                })
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_wheel_drag(cx)))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_wheel_drag(cx)));
            let centre = {
                let state = self.state.clone();
                let effect_id = effect_id.to_owned();
                Button::new(eid("colour-wheel-centre", &key), t("Centre"))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::XSmall)
                    .on_click(move |_, _, cx| {
                        let next = editor::colour_wheel_move(offsets, 0.0, 0.0);
                        let mut params = Params::new();
                        for (key, value) in keys.iter().zip(next) {
                            params.set(*key, value);
                        }
                        state.update(cx, |state, cx| state.update_effect(&effect_id, Some(params), None, cx));
                    })
            };
            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .child(wheel_disc)
                    .child(div().text_xs().text_color(colors.muted_foreground).child(t(label)))
                    .child(centre),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(row)
            .child(div().text_xs().text_color(colors.muted_foreground).child(t("Drag a dot to tint the shadows, midtones or highlights; the rows below show the numbers.")))
    }

    /// Where a pointer position falls on a wheel (unit disc, y up).
    fn wheel_point_at(&self, key: &str, position: Point<Pixels>) -> (f64, f64) {
        let Some(bounds) = self.wheel_bounds.borrow().get(key).copied() else { return (0.0, 0.0) };
        let half = f32::from(bounds.size.width) / 2.0;
        if half <= 0.0 {
            return (0.0, 0.0);
        }
        let x = (f32::from(position.x - bounds.origin.x) - half) / half;
        let y = (half - f32::from(position.y - bounds.origin.y)) / half;
        (x as f64, y as f64)
    }

    /// Writes a dragged wheel's offsets (one undo step).
    fn finish_wheel_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.wheel_drag.take() else { return };
        let keys = editor::COLOUR_WHEELS[drag.wheel].1;
        let next = editor::colour_wheel_move(drag.base, drag.point.0, drag.point.1);
        let mut params = Params::new();
        for (key, value) in keys.iter().zip(next) {
            params.set(*key, (value * 1000.0).round() / 1000.0);
        }
        let effect_id = drag.effect_id;
        self.state.update(cx, |state, cx| state.update_effect(&effect_id, Some(params), None, cx));
        cx.notify();
    }
}

/// A hue's fully saturated colour.
fn hue_colour(hue: f64) -> [u8; 3] {
    let hp = hue.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (hp % 2.0 - 1.0).abs();
    let (r, g, b) = match hp as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 255.0f64).round() as u8, (g * 255.0f64).round() as u8, (b * 255.0f64).round() as u8]
}

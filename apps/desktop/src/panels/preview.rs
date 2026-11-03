use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, Context, Entity, MouseButton, ObjectFit, Pixels, Window, canvas, div, img, prelude::*, px};

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
}

impl PreviewPanel {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            frame_bounds: Rc::new(Cell::new(Bounds::default())),
        }
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

        let bounds_cell = Rc::clone(&self.frame_bounds);
        let frame_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .bg(gpui::black())
            .on_mouse_down(MouseButton::Left, {
                let focus_handle = focus_handle.clone();
                move |_, window, _| window.focus(&focus_handle)
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

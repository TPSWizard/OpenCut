use gpui::{div, prelude::*, px, Context, SharedString, Window};
use time::{FormatTimecodeOptions, FrameRate, MediaTime, TimeCodeFormat, format_timecode};

use crate::theme::ActiveTheme;

/// Placeholder sequence settings until projects exist on desktop.
const SEQUENCE_RATE: FrameRate = FrameRate::FPS_30;
const SEQUENCE_SECONDS: f64 = 60.0;
const RULER_STEP_SECONDS: f64 = 5.0;

pub(crate) struct Timeline;

struct RulerTick {
    label: SharedString,
}

fn ruler_ticks() -> Vec<RulerTick> {
    let step_count = (SEQUENCE_SECONDS / RULER_STEP_SECONDS) as usize;

    (0..step_count)
        .filter_map(|index| {
            let seconds = index as f64 * RULER_STEP_SECONDS;
            let time = MediaTime::from_seconds_f64(seconds)?.floor_to_frame(SEQUENCE_RATE)?;
            let label = format_timecode(FormatTimecodeOptions {
                time,
                format: Some(TimeCodeFormat::MmSs),
                rate: Some(SEQUENCE_RATE),
            })?;

            Some(RulerTick {
                label: SharedString::from(label),
            })
        })
        .collect()
}

fn sequence_summary() -> SharedString {
    let duration = MediaTime::from_seconds_f64(SEQUENCE_SECONDS);
    let frames = duration
        .and_then(|time| time.to_frame_floor(SEQUENCE_RATE))
        .unwrap_or(0);
    let fps = SEQUENCE_RATE.as_f64().unwrap_or(0.0);
    let end = duration
        .and_then(|time| time.last_frame_time(SEQUENCE_RATE))
        .and_then(|time| {
            format_timecode(FormatTimecodeOptions {
                time,
                format: Some(TimeCodeFormat::HhMmSsFf),
                rate: Some(SEQUENCE_RATE),
            })
        })
        .unwrap_or_default();

    SharedString::from(format!("{fps:.0} fps  ·  {frames} frames  ·  last frame {end}"))
}

impl Render for Timeline {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;

        let ruler = div()
            .flex()
            .flex_row()
            .w_full()
            .h(px(24.))
            .border_b_1()
            .border_color(colors.border)
            .text_xs()
            .text_color(colors.muted_foreground)
            .children(ruler_ticks().into_iter().map(|tick| {
                div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .items_end()
                    .border_l_1()
                    .border_color(colors.border)
                    .px_1()
                    .child(tick.label)
            }));

        div()
            .flex()
            .flex_col()
            .h_1_3()
            .bg(colors.card)
            .text_color(colors.card_foreground)
            .child(ruler)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(colors.muted_foreground)
                    .child(sequence_summary()),
            )
    }
}

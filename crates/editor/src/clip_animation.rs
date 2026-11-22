//! One-click animations for a clip: a way in over its first moments, a way
//! out over its last, and a loop over the whole clip. They are worked out
//! while the clip plays, on top of its keyframes, from six element params:
//! `animIn` / `animInDuration`, `animOut` / `animOutDuration` and
//! `animLoop` / `animLoopPeriod`. So they follow the clip when it is
//! trimmed or moved and never clash with keyframes. The web editor's
//! `@/clip-animation/presets` is the same maths.

use crate::Params;
use std::f64::consts::PI;

pub const ANIMATION_IN_KINDS: &[&str] = &[
    "fade", "pop", "grow", "shrink", "slideUp", "slideDown", "slideLeft", "slideRight", "bounce", "spin",
    "typewriter", "wordByWord",
];
pub const ANIMATION_OUT_KINDS: &[&str] =
    &["fade", "pop", "grow", "shrink", "slideUp", "slideDown", "slideLeft", "slideRight", "spin"];
pub const ANIMATION_LOOP_KINDS: &[&str] = &["pulse", "shake", "wobble", "float", "kenBurns"];
/// Presets that reveal text letter by letter or word by word.
pub const TEXT_ONLY_ANIMATIONS: &[&str] = &["typewriter", "wordByWord"];

pub const ANIMATION_DURATION_DEFAULT: f64 = 0.5;
pub const ANIMATION_DURATION_MIN: f64 = 0.1;
pub const ANIMATION_DURATION_MAX: f64 = 5.0;
pub const ANIMATION_LOOP_PERIOD_DEFAULT: f64 = 1.0;
pub const ANIMATION_LOOP_PERIOD_MIN: f64 = 0.2;
pub const ANIMATION_LOOP_PERIOD_MAX: f64 = 10.0;

/// A preset's name for people, as the web editor shows it.
pub fn animation_label(kind: &str) -> &'static str {
    match kind {
        "fade" => "Fade",
        "pop" => "Pop",
        "grow" => "Grow",
        "shrink" => "Shrink",
        "slideUp" => "Slide up",
        "slideDown" => "Slide down",
        "slideLeft" => "Slide left",
        "slideRight" => "Slide right",
        "bounce" => "Drop and bounce",
        "spin" => "Spin",
        "typewriter" => "Typewriter",
        "wordByWord" => "Word by word",
        "pulse" => "Pulse",
        "shake" => "Shake",
        "wobble" => "Wobble",
        "float" => "Float",
        "kenBurns" => "Ken Burns",
        _ => "None",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipAnimations {
    /// Kind and seconds.
    pub animation_in: Option<(String, f64)>,
    pub animation_out: Option<(String, f64)>,
    /// Kind and seconds per cycle.
    pub animation_loop: Option<(String, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealMode {
    Chars,
    Words,
}

/// How a clip looks at one moment, relative to its keyframed state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipAnimationState {
    /// Multiplies the opacity.
    pub opacity: f64,
    /// Multiplies both scales.
    pub scale: f64,
    /// Canvas pixels added to the position.
    pub offset_x: f64,
    pub offset_y: f64,
    /// Degrees added to the rotation.
    pub rotate: f64,
    /// How much of the text shows; `None` when all of it does.
    pub reveal: Option<(RevealMode, f64)>,
}

impl Default for ClipAnimationState {
    fn default() -> Self {
        Self { opacity: 1.0, scale: 1.0, offset_x: 0.0, offset_y: 0.0, rotate: 0.0, reveal: None }
    }
}

fn read_number(params: &Params, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    let value = params.number(key, fallback);
    if value.is_finite() { value.clamp(min, max) } else { fallback }
}

fn read_kind(params: &Params, key: &str, kinds: &[&str]) -> Option<String> {
    let kind = params.text(key, "none");
    kinds.contains(&kind.as_ref()).then(|| kind.into_owned())
}

/// The clip's animations from its params, or `None` when it has none.
pub fn read_clip_animations(params: &Params) -> Option<ClipAnimations> {
    let duration = |key: &str| {
        read_number(params, key, ANIMATION_DURATION_DEFAULT, ANIMATION_DURATION_MIN, ANIMATION_DURATION_MAX)
    };
    let animations = ClipAnimations {
        animation_in: read_kind(params, "animIn", ANIMATION_IN_KINDS).map(|kind| (kind, duration("animInDuration"))),
        animation_out: read_kind(params, "animOut", ANIMATION_OUT_KINDS)
            .map(|kind| (kind, duration("animOutDuration"))),
        animation_loop: read_kind(params, "animLoop", ANIMATION_LOOP_KINDS).map(|kind| {
            (
                kind,
                read_number(
                    params,
                    "animLoopPeriod",
                    ANIMATION_LOOP_PERIOD_DEFAULT,
                    ANIMATION_LOOP_PERIOD_MIN,
                    ANIMATION_LOOP_PERIOD_MAX,
                ),
            )
        }),
    };
    (animations.animation_in.is_some() || animations.animation_out.is_some() || animations.animation_loop.is_some())
        .then_some(animations)
}

fn ease_out_cubic(p: f64) -> f64 {
    1.0 - (1.0 - p).powi(3)
}

fn ease_out_back(p: f64) -> f64 {
    const C1: f64 = 1.70158;
    const C3: f64 = C1 + 1.0;
    1.0 + C3 * (p - 1.0).powi(3) + C1 * (p - 1.0).powi(2)
}

fn ease_in_out_sine(p: f64) -> f64 {
    -((PI * p).cos() - 1.0) / 2.0
}

fn ease_out_bounce(p: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if p < 1.0 / D1 {
        N1 * p * p
    } else if p < 2.0 / D1 {
        let q = p - 1.5 / D1;
        N1 * q * q + 0.75
    } else if p < 2.5 / D1 {
        let q = p - 2.25 / D1;
        N1 * q * q + 0.9375
    } else {
        let q = p - 2.625 / D1;
        N1 * q * q + 0.984375
    }
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

/// Share of the canvas a slide travels, and of its height a drop falls from.
const SLIDE_DISTANCE: f64 = 0.3;

/// An in or out preset at `progress` (0 = gone, 1 = fully in place).
fn apply_edge(
    state: &mut ClipAnimationState,
    kind: &str,
    is_in: bool,
    progress: f64,
    (canvas_width, canvas_height): (f64, f64),
    is_text: bool,
) {
    let v = progress.clamp(0.0, 1.0);
    let eased = ease_out_cubic(v);
    let side = if is_in { 1.0 } else { -1.0 };
    let slide_x = SLIDE_DISTANCE * canvas_width * (1.0 - eased);
    let slide_y = SLIDE_DISTANCE * canvas_height * (1.0 - eased);
    match kind {
        "fade" => state.opacity *= v,
        "pop" => {
            state.scale *= ease_out_back(v).max(0.0);
            state.opacity *= (4.0 * v).min(1.0);
        }
        "grow" => {
            state.scale *= lerp(if is_in { 0.5 } else { 1.6 }, 1.0, eased);
            state.opacity *= v;
        }
        "shrink" => {
            state.scale *= lerp(if is_in { 1.6 } else { 0.5 }, 1.0, eased);
            state.opacity *= v;
        }
        "slideUp" | "slideDown" | "slideLeft" | "slideRight" => {
            match kind {
                "slideUp" => state.offset_y += side * slide_y,
                "slideDown" => state.offset_y -= side * slide_y,
                "slideLeft" => state.offset_x += side * slide_x,
                _ => state.offset_x -= side * slide_x,
            }
            state.opacity *= (2.0 * v).min(1.0);
        }
        "bounce" => {
            state.offset_y -= SLIDE_DISTANCE * canvas_height * (1.0 - ease_out_bounce(v));
            state.opacity *= (4.0 * v).min(1.0);
        }
        "spin" => {
            state.rotate -= side * 180.0 * (1.0 - eased);
            state.scale *= lerp(0.3, 1.0, eased);
            state.opacity *= v;
        }
        "typewriter" | "wordByWord" => {
            if is_text {
                let mode = if kind == "typewriter" { RevealMode::Chars } else { RevealMode::Words };
                state.reveal = Some((mode, v));
            } else {
                state.opacity *= v;
            }
        }
        _ => {}
    }
}

fn apply_loop(
    state: &mut ClipAnimationState,
    kind: &str,
    period: f64,
    local: f64,
    length: f64,
    (canvas_width, canvas_height): (f64, f64),
) {
    let angle = 2.0 * PI * local / period;
    match kind {
        "pulse" => state.scale *= 1.0 + 0.04 * (1.0 - angle.cos()),
        "shake" => {
            state.offset_x += 0.01 * canvas_width * (0.6 * (3.0 * angle).sin() + 0.4 * (7.0 * angle).sin());
            state.offset_y += 0.01 * canvas_height * (0.6 * (4.0 * angle).sin() + 0.4 * (9.0 * angle).sin());
        }
        "wobble" => state.rotate += 6.0 * angle.sin(),
        "float" => state.offset_y -= 0.02 * canvas_height * angle.sin(),
        "kenBurns" => {
            let eased = ease_in_out_sine(if length > 0.0 { (local / length).clamp(0.0, 1.0) } else { 0.0 });
            state.scale *= 1.0 + 0.15 * eased;
            state.offset_x -= 0.04 * canvas_width * eased;
            state.offset_y -= 0.02 * canvas_height * eased;
        }
        _ => {}
    }
}

/// The in and out windows in seconds, scaled down together when they would
/// overlap on a clip `length` seconds long.
pub fn animation_windows(animations: &ClipAnimations, length: f64) -> (f64, f64) {
    let mut seconds_in = animations.animation_in.as_ref().map_or(0.0, |(_, seconds)| *seconds);
    let mut seconds_out = animations.animation_out.as_ref().map_or(0.0, |(_, seconds)| *seconds);
    let length = length.max(0.0);
    if seconds_in + seconds_out > length && seconds_in + seconds_out > 0.0 {
        let scale = length / (seconds_in + seconds_out);
        seconds_in *= scale;
        seconds_out *= scale;
    }
    (seconds_in, seconds_out)
}

/// The clip's animated state `local` seconds into a clip `length` seconds
/// long on a canvas of `canvas` pixels.
pub fn clip_animation_state_at(
    animations: &ClipAnimations,
    local: f64,
    length: f64,
    canvas: (f64, f64),
    is_text: bool,
) -> ClipAnimationState {
    let mut state = ClipAnimationState::default();
    let local = local.max(0.0).min(length.max(0.0));
    let (seconds_in, seconds_out) = animation_windows(animations, length);
    if let Some((kind, period)) = &animations.animation_loop {
        apply_loop(&mut state, kind, *period, local, length, canvas);
    }
    if let Some((kind, _)) = &animations.animation_in {
        if seconds_in > 0.0 && local < seconds_in {
            apply_edge(&mut state, kind, true, local / seconds_in, canvas, is_text);
        }
    }
    if let Some((kind, _)) = &animations.animation_out {
        if seconds_out > 0.0 && local > length - seconds_out {
            apply_edge(&mut state, kind, false, (length - local) / seconds_out, canvas, is_text);
        }
    }
    state
}

/// How many characters of `content` show (newlines not counted), or `None`
/// when all of them do. Words are runs of non-space characters; a word
/// shows whole, with the spaces before it. The web editor's
/// `revealedCharCount` is the same.
pub fn revealed_char_count(content: &str, reveal: Option<(RevealMode, f64)>) -> Option<usize> {
    let (mode, fraction) = reveal?;
    if fraction >= 1.0 {
        return None;
    }
    let fraction = fraction.clamp(0.0, 1.0);
    let chars: Vec<char> = content.chars().collect();
    if mode == RevealMode::Chars {
        let count = chars.iter().filter(|c| **c != '\n').count();
        return Some((fraction * count as f64).ceil() as usize);
    }
    let mut word_ends = Vec::new();
    let mut counted = 0;
    for (index, c) in chars.iter().enumerate() {
        if *c == '\n' {
            continue;
        }
        counted += 1;
        let next_is_space = chars.get(index + 1).is_none_or(|next| next.is_whitespace());
        if !c.is_whitespace() && next_is_space {
            word_ends.push(counted);
        }
    }
    let shown = (fraction * word_ends.len() as f64).ceil() as usize;
    Some(if shown == 0 { 0 } else { word_ends[shown - 1] })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS: (f64, f64) = (1920.0, 1080.0);

    fn params(pairs: &[(&str, &str)], numbers: &[(&str, f64)]) -> Params {
        let mut params = Params::default();
        for (key, value) in pairs {
            params.set(*key, *value);
        }
        for (key, value) in numbers {
            params.set(*key, *value);
        }
        params
    }

    #[test]
    fn reads_and_matches_the_web_maths() {
        assert!(read_clip_animations(&params(&[("animIn", "none"), ("animLoop", "wiggle")], &[])).is_none());
        let fade = read_clip_animations(&params(&[("animIn", "fade")], &[("animInDuration", 1.0)])).unwrap();
        assert_eq!(clip_animation_state_at(&fade, 0.0, 4.0, CANVAS, false).opacity, 0.0);
        assert!((clip_animation_state_at(&fade, 0.5, 4.0, CANVAS, false).opacity - 0.5).abs() < 1e-9);
        assert_eq!(clip_animation_state_at(&fade, 3.0, 4.0, CANVAS, false), ClipAnimationState::default());

        let slide = read_clip_animations(&params(&[("animIn", "slideUp"), ("animOut", "slideUp")], &[])).unwrap();
        assert!((clip_animation_state_at(&slide, 0.0, 4.0, CANVAS, false).offset_y - 324.0).abs() < 1e-9);
        assert!((clip_animation_state_at(&slide, 4.0, 4.0, CANVAS, false).offset_y + 324.0).abs() < 1e-9);

        let both = read_clip_animations(&params(
            &[("animIn", "fade"), ("animOut", "fade")],
            &[("animInDuration", 2.0), ("animOutDuration", 2.0)],
        ))
        .unwrap();
        assert_eq!(animation_windows(&both, 1.0), (0.5, 0.5));

        let ken_burns = read_clip_animations(&params(&[("animLoop", "kenBurns")], &[])).unwrap();
        let end = clip_animation_state_at(&ken_burns, 5.0, 5.0, CANVAS, false);
        assert!((end.scale - 1.15).abs() < 1e-9 && (end.offset_x + 76.8).abs() < 1e-9);

        let pop = read_clip_animations(&params(&[("animIn", "pop")], &[("animInDuration", 1.0)])).unwrap();
        assert!(clip_animation_state_at(&pop, 0.7, 3.0, CANVAS, false).scale > 1.05);
    }

    #[test]
    fn reveals_text_letter_by_letter_or_word_by_word() {
        let typewriter = read_clip_animations(&params(&[("animIn", "typewriter")], &[("animInDuration", 1.0)])).unwrap();
        let state = clip_animation_state_at(&typewriter, 0.5, 3.0, CANVAS, true);
        assert_eq!(state.reveal, Some((RevealMode::Chars, 0.5)));
        assert_eq!(clip_animation_state_at(&typewriter, 0.5, 3.0, CANVAS, false).opacity, 0.5);
        assert_eq!(revealed_char_count("ab\ncd", Some((RevealMode::Chars, 0.5))), Some(2));
        assert_eq!(revealed_char_count("ab\ncd", Some((RevealMode::Chars, 1.0))), None);
        let words = |fraction| revealed_char_count("one two\nthree", Some((RevealMode::Words, fraction)));
        assert_eq!(words(0.0), Some(0));
        assert_eq!(words(0.2), Some(3));
        assert_eq!(words(0.5), Some(7));
        assert_eq!(words(0.9), Some(12));
    }
}

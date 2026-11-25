//! Audio crossfades between neighbouring clips on one track.
//!
//! A crossfade belongs to the clip it leads out of (its `crossfade` param,
//! in seconds, and `crossfadeCurve`) and joins it to the next clip that
//! starts where it ends (within 10 ms). The timeline itself never overlaps:
//! for playback both clips are stretched over the cut into their spare
//! source audio (the "handles": the outgoing clip's `trimEnd`, the
//! incoming one's `trimStart`), half each way when both have enough, so the
//! two play together for the crossfade's length with the outgoing one
//! fading out and the incoming one fading in on the chosen curve. Equal
//! power keeps the loudness steady through the join (`sin² + cos² = 1`);
//! linear is a straight ramp.
//!
//! Handles that are too short shorten the join (none at all leaves a plain
//! cut), so the one-step command ([`plan`]) first trims the clips to make
//! room: the outgoing clip's end and the incoming clip's start move in by
//! what each is short of, and the incoming clip and everything after it on
//! the track move left to close the gap. Nothing is lost: the trimmed audio
//! is what plays under the crossfade.
//!
//! All times are ticks (120 000 a second), as both editors store them, so
//! the two compute exactly the same joins. Clips with a speed change get
//! no crossfade. Checked against `fixtures/crossfade/cases.json`.

use serde::{Deserialize, Serialize};

pub const TICKS_PER_SECOND: i64 = 120_000;
/// The longest crossfade, 10 s, as for clip fades.
pub const MAX_TICKS: i64 = 10 * TICKS_PER_SECOND;
/// How far apart two clips may be and still count as one cut: 10 ms.
pub const ADJACENCY_TICKS: i64 = 1_200;
/// The shortest crossfade the command makes, 10 ms.
pub const MIN_TICKS: i64 = 1_200;

/// How the gains cross.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    /// A straight ramp; the sum of the two gains stays 1 (a slight dip in
    /// loudness at the middle of the join).
    #[default]
    Linear,
    /// `sin` in, `cos` out: steady loudness through the join.
    EqualPower,
}

impl Curve {
    /// The curve a param names; anything unknown is linear.
    pub fn from_param(value: &str) -> Curve {
        if value == "equalPower" { Curve::EqualPower } else { Curve::Linear }
    }

    pub fn as_param(self) -> &'static str {
        match self {
            Curve::Linear => "linear",
            Curve::EqualPower => "equalPower",
        }
    }

    /// A fade's gain at `progress` (0 silent, 1 full) through it.
    pub fn gain(self, progress: f64) -> f64 {
        let progress = if progress.is_finite() { progress.clamp(0.0, 1.0) } else { 0.0 };
        match self {
            Curve::Linear => progress,
            Curve::EqualPower => (progress * std::f64::consts::FRAC_PI_2).sin(),
        }
    }
}

/// One clip on a track, as both editors store it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackClip {
    pub start: i64,
    pub duration: i64,
    /// Source before the part that plays (the incoming handle).
    pub trim_start: i64,
    /// Source after the part that plays (the outgoing handle).
    pub trim_end: i64,
    /// A speed change, ramp or reverse: no crossfade either side.
    pub retimed: bool,
    /// Whether it can take part: a visible audio or video clip.
    pub eligible: bool,
    /// Its own fades in seconds (already within the editors' limits).
    pub fade_in: f64,
    pub fade_out: f64,
    /// Its crossfade into the next clip, ticks; 0 for none.
    pub crossfade: i64,
    pub curve: Curve,
}

/// A clip as it plays once its crossfades are applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipAudio {
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub fade_in_curve: Curve,
    pub fade_out_curve: Curve,
}

/// A crossfade as it plays: clip `from` runs `right` ticks past the cut,
/// clip `to` starts `left` ticks before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Join {
    pub from: usize,
    pub to: usize,
    pub left: i64,
    pub right: i64,
    pub curve: Curve,
}

impl Join {
    pub fn length(&self) -> i64 {
        self.left + self.right
    }
}

/// The clip that follows `index` directly on the track (starting within
/// 10 ms of its end), if both can take part.
pub fn next_neighbour(clips: &[TrackClip], index: usize) -> Option<usize> {
    let clip = clips.get(index)?;
    if !clip.eligible || clip.duration <= 0 {
        return None;
    }
    let end = clip.start + clip.duration;
    clips
        .iter()
        .enumerate()
        .filter(|(other, candidate)| {
            *other != index && candidate.eligible && candidate.duration > 0 && (candidate.start - end).abs() <= ADJACENCY_TICKS
        })
        .min_by_key(|(other, candidate)| (candidate.start, *other))
        .map(|(other, _)| other)
}

/// Clip indices in timeline order.
fn order(clips: &[TrackClip]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..clips.len()).collect();
    indices.sort_by_key(|&index| (clips[index].start, index));
    indices
}

/// How the join from `from` to `to` plays for a crossfade of `ticks`,
/// given `used` ticks of `from`'s start already under the join into it.
fn join_of(clips: &[TrackClip], from: usize, to: usize, ticks: i64, used: i64) -> Option<(i64, i64)> {
    let (a, b) = (&clips[from], &clips[to]);
    if a.retimed || b.retimed {
        return None;
    }
    let length = ticks.min(MAX_TICKS).min(a.duration - used).min(b.duration);
    if length <= 0 {
        return None;
    }
    let (handle_out, handle_in) = (a.trim_end.max(0), b.trim_start.max(0));
    // Centred on the cut when both have the handle, else leaning on the side that does.
    let mut right = (length - length / 2).min(handle_out);
    let left = (length - right).min(handle_in);
    right = (length - left).min(handle_out);
    (left + right > 0).then_some((left, right))
}

/// Every crossfade on the track as it plays, in timeline order. A clip
/// whose start is already under the join into it has only the rest of its
/// length for its own.
pub fn joins(clips: &[TrackClip]) -> Vec<Join> {
    let mut used = vec![0i64; clips.len()];
    let mut found = Vec::new();
    for from in order(clips) {
        let clip = &clips[from];
        if clip.crossfade <= 0 {
            continue;
        }
        let Some(to) = next_neighbour(clips, from) else { continue };
        if let Some((left, right)) = join_of(clips, from, to, clip.crossfade, used[from]) {
            used[to] = right;
            found.push(Join { from, to, left, right, curve: clip.curve });
        }
    }
    found
}

/// The clips as they play with their crossfades: `None` for a clip no
/// crossfade touches. A joined clip is stretched over the cut and fades on
/// the crossfade's curve; its own fade on the other side is kept, shortened
/// to fit.
pub fn crossfade_track(clips: &[TrackClip]) -> Vec<Option<ClipAudio>> {
    let mut out: Vec<Option<ClipAudio>> = vec![None; clips.len()];
    let mut joined_in = vec![false; clips.len()];
    let mut joined_out = vec![false; clips.len()];
    let fresh = |clip: &TrackClip| ClipAudio {
        start: clip.start,
        duration: clip.duration,
        trim_start: clip.trim_start,
        trim_end: clip.trim_end,
        fade_in: clip.fade_in,
        fade_out: clip.fade_out,
        fade_in_curve: Curve::Linear,
        fade_out_curve: Curve::Linear,
    };
    for join in joins(clips) {
        let seconds = join.length() as f64 / TICKS_PER_SECOND as f64;
        let a = out[join.from].get_or_insert_with(|| fresh(&clips[join.from]));
        a.duration += join.right;
        a.trim_end -= join.right;
        a.fade_out = seconds;
        a.fade_out_curve = join.curve;
        joined_out[join.from] = true;
        let b = out[join.to].get_or_insert_with(|| fresh(&clips[join.to]));
        b.start -= join.left;
        b.duration += join.left;
        b.trim_start -= join.left;
        b.fade_in = seconds;
        b.fade_in_curve = join.curve;
        joined_in[join.to] = true;
    }
    for (index, clip) in out.iter_mut().enumerate() {
        let Some(clip) = clip else { continue };
        let length = clip.duration as f64 / TICKS_PER_SECOND as f64;
        if joined_in[index] && !joined_out[index] {
            clip.fade_out = clip.fade_out.min(length - clip.fade_in).max(0.0);
        }
        if joined_out[index] && !joined_in[index] {
            clip.fade_in = clip.fade_in.min(length - clip.fade_out).max(0.0);
        }
    }
    out
}

/// Why a crossfade cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanError {
    /// No audio or video clip starts where this one ends on its track.
    NoNeighbour,
    /// One of the two clips has a speed change.
    Retimed,
    /// The clips are too short for even a 10 ms crossfade.
    TooShort,
}

impl PlanError {
    pub fn message(self) -> &'static str {
        match self {
            PlanError::NoNeighbour => "No audio or video clip starts where this one ends on its track",
            PlanError::Retimed => "Crossfades need both clips at normal speed",
            PlanError::TooShort => "The clips are too short to crossfade",
        }
    }
}

/// Where a clip goes: its new start, length and trims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipEdit {
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
}

/// The one-step crossfade from a clip into the next: the length it gets,
/// the trims that make room for it, and the clips that move left.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// The incoming clip's index.
    pub to: usize,
    /// The crossfade to store on the outgoing clip, ticks.
    pub length: i64,
    /// How much of the outgoing clip's end and the incoming clip's start
    /// is trimmed to make handles.
    pub trim_out: i64,
    pub trim_in: i64,
    /// How far the incoming clip and those after it move left.
    pub shift: i64,
    pub from_edit: ClipEdit,
    pub to_edit: ClipEdit,
    /// The other clips after the incoming one, in timeline order: each
    /// moves `shift` earlier.
    pub moved: Vec<usize>,
}

/// The longest crossfade one side can give when it may be trimmed for
/// handle: `length` of clip with `handle` spare (half the crossfade comes
/// from the handle; what it lacks is trimmed off the clip).
fn room(length: i64, handle: i64) -> i64 {
    let handle = handle.max(0);
    if length <= 2 * handle { length } else { 2 * (length + handle) / 3 }
}

/// The one-step crossfade of `ticks` from clip `index` into the clip after
/// it: as long as asked (at most 10 s) if the clips allow, trimmed to make
/// handles where they are short.
pub fn plan(clips: &[TrackClip], index: usize, ticks: i64) -> Result<Plan, PlanError> {
    let to = next_neighbour(clips, index).ok_or(PlanError::NoNeighbour)?;
    let (a, b) = (&clips[index], &clips[to]);
    if a.retimed || b.retimed {
        return Err(PlanError::Retimed);
    }
    // The start of this clip already under a crossfade into it.
    let used = joins(clips).iter().find(|join| join.to == index).map_or(0, |join| join.right);
    let length = ticks.min(MAX_TICKS).min(room(a.duration - used, a.trim_end)).min(room(b.duration, b.trim_start));
    if length < MIN_TICKS {
        return Err(PlanError::TooShort);
    }
    let left = length / 2;
    let right = length - left;
    let trim_out = (right - a.trim_end.max(0)).max(0);
    let trim_in = (left - b.trim_start.max(0)).max(0);
    let shift = trim_out + trim_in;
    let from_edit = ClipEdit { start: a.start, duration: a.duration - trim_out, trim_start: a.trim_start, trim_end: a.trim_end + trim_out };
    let to_edit = ClipEdit {
        start: b.start + trim_in - shift,
        duration: b.duration - trim_in,
        trim_start: b.trim_start + trim_in,
        trim_end: b.trim_end,
    };
    let moved = if shift > 0 {
        order(clips).into_iter().filter(|&other| other != index && other != to && clips[other].start > b.start).collect()
    } else {
        Vec::new()
    };
    Ok(Plan { to, length, trim_out, trim_in, shift, from_edit, to_edit, moved })
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = TICKS_PER_SECOND;

    fn clip(start: f64, duration: f64, trim_start: f64, trim_end: f64) -> TrackClip {
        TrackClip {
            start: (start * S as f64) as i64,
            duration: (duration * S as f64) as i64,
            trim_start: (trim_start * S as f64) as i64,
            trim_end: (trim_end * S as f64) as i64,
            eligible: true,
            ..Default::default()
        }
    }

    #[test]
    fn equal_power_keeps_the_power_steady() {
        for step in 0..=10 {
            let p = step as f64 / 10.0;
            let (fade_in, fade_out) = (Curve::EqualPower.gain(p), Curve::EqualPower.gain(1.0 - p));
            assert!((fade_in * fade_in + fade_out * fade_out - 1.0).abs() < 1e-12);
            assert!((Curve::Linear.gain(p) + Curve::Linear.gain(1.0 - p) - 1.0).abs() < 1e-12);
        }
        assert_eq!(Curve::from_param("equalPower"), Curve::EqualPower);
        assert_eq!(Curve::from_param("bogus"), Curve::Linear);
    }

    #[test]
    fn centred_on_the_cut_with_handles() {
        let mut clips = vec![clip(0.0, 4.0, 0.0, 2.0), clip(4.0, 4.0, 2.0, 0.0)];
        clips[0].crossfade = S;
        clips[0].curve = Curve::EqualPower;
        let played = crossfade_track(&clips);
        let a = played[0].as_ref().unwrap();
        let b = played[1].as_ref().unwrap();
        assert_eq!((a.duration, a.trim_end, a.fade_out), (4 * S + S / 2, 2 * S - S / 2, 1.0));
        assert_eq!((b.start, b.duration, b.trim_start, b.fade_in), (4 * S - S / 2, 4 * S + S / 2, 2 * S - S / 2, 1.0));
        assert_eq!(a.fade_out_curve, Curve::EqualPower);
    }

    #[test]
    fn without_handles_the_plan_trims_and_closes_the_gap() {
        let mut clips = vec![clip(0.0, 4.0, 0.0, 0.0), clip(4.0, 4.0, 0.0, 0.0), clip(8.0, 2.0, 0.0, 0.0)];
        let plan = plan(&clips, 0, S).unwrap();
        assert_eq!((plan.length, plan.trim_out, plan.trim_in, plan.shift), (S, S / 2, S / 2, S));
        assert_eq!(plan.from_edit, ClipEdit { start: 0, duration: 3 * S + S / 2, trim_start: 0, trim_end: S / 2 });
        assert_eq!(plan.to_edit, ClipEdit { start: 3 * S + S / 2, duration: 3 * S + S / 2, trim_start: S / 2, trim_end: 0 });
        assert_eq!(plan.moved, vec![2]);
        // Applied, the join plays the full second.
        clips[0] = TrackClip { crossfade: plan.length, ..clip(0.0, 3.5, 0.0, 0.5) };
        clips[1] = clip(3.5, 3.5, 0.5, 0.0);
        assert_eq!(joins(&clips), vec![Join { from: 0, to: 1, left: S / 2, right: S / 2, curve: Curve::Linear }]);
    }

    #[test]
    fn no_neighbour_or_speed_change_is_refused() {
        let mut clips = vec![clip(0.0, 4.0, 0.0, 0.0), clip(5.0, 4.0, 0.0, 0.0)];
        assert_eq!(plan(&clips, 0, S), Err(PlanError::NoNeighbour));
        clips[1].start = 4 * S;
        clips[1].retimed = true;
        assert_eq!(plan(&clips, 0, S), Err(PlanError::Retimed));
    }
}

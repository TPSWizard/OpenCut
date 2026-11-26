//! Slip, slide and roll edits (row 93), the three trim tools every pro
//! editor has, worked out the same for both editors (the web through
//! WASM). All times are ticks.
//!
//! - **Slip** moves what a clip shows inside it: its place on the timeline
//!   stays, its trims move the same way at both ends.
//! - **Slide** moves a clip between its neighbours: the one before grows or
//!   shrinks at its end and the one after at its start, so nothing else
//!   moves. Without a neighbour on a side the clip slides into the gap
//!   there, never past it.
//! - **Roll** moves the cut between two clips that touch: the first ends
//!   later or sooner and the second starts the same.
//!
//! A clip from media (video, audio) cannot show more than its media holds
//! (`limited`: trims stay at or above zero); stills, text and graphics can
//! grow freely (their trims are left alone). No clip gets shorter than
//! `min_duration`. The asked-for move is clamped to what fits, and the
//! clamped move comes back with the changes. Clips with a speed change are
//! refused (their trims are in source time). Checked against
//! `fixtures/trimedit/cases.json` (an independent reference).

use serde::{Deserialize, Serialize};

/// A clip as the tools see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
    /// From media: it cannot show more than the media holds.
    #[serde(default)]
    pub limited: bool,
    /// Has a speed change (refused).
    #[serde(default)]
    pub retimed: bool,
}

/// A clip's new place and trims.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub id: String,
    pub start: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub trim_end: i64,
}

/// What an edit does: the move that fitted and each clip's change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub delta: i64,
    pub changes: Vec<Change>,
}

/// Which tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Slip,
    Slide,
    Roll,
}

impl Tool {
    pub fn parse(id: &str) -> Option<Tool> {
        match id {
            "slip" => Some(Tool::Slip),
            "slide" => Some(Tool::Slide),
            "roll" => Some(Tool::Roll),
            _ => None,
        }
    }
}

fn change(clip: &Clip, start: i64, duration: i64, trim_start: i64, trim_end: i64) -> Change {
    Change { id: clip.id.clone(), start, duration, trim_start, trim_end }
}

fn refuse_retimed(clips: &[&Clip]) -> Result<(), String> {
    if clips.iter().any(|clip| clip.retimed) {
        return Err("Slip, slide and roll do not work on clips with a speed change".to_owned());
    }
    Ok(())
}

/// Clamps `delta` to `[low, high]` (an empty range gives 0).
fn fit(delta: i64, low: i64, high: i64) -> i64 {
    if low > high { 0 } else { delta.clamp(low, high) }
}

/// Slips `clip` by `delta` (positive shows later media).
pub fn slip(clip: &Clip, delta: i64) -> Result<Plan, String> {
    refuse_retimed(&[clip])?;
    if !clip.limited {
        return Err("Only video and audio clips can slip (they show part of their media)".to_owned());
    }
    let delta = fit(delta, -clip.trim_start, clip.trim_end);
    Ok(Plan {
        delta,
        changes: vec![change(clip, clip.start, clip.duration, clip.trim_start + delta, clip.trim_end - delta)],
    })
}

/// The changes when `left` (ending at the cut) ends `delta` later.
fn grow_end(left: &Clip, delta: i64) -> Change {
    let trim_end = if left.limited { left.trim_end - delta } else { left.trim_end };
    change(left, left.start, left.duration + delta, left.trim_start, trim_end)
}

/// The changes when `right` (starting at the cut) starts `delta` later.
fn move_start(right: &Clip, delta: i64) -> Change {
    let trim_start = if right.limited { right.trim_start + delta } else { right.trim_start };
    change(right, right.start + delta, right.duration - delta, trim_start, right.trim_end)
}

/// How far a clip ending at a cut may move its end: `(low, high)`.
fn end_range(left: &Clip, min_duration: i64) -> (i64, i64) {
    let high = if left.limited { left.trim_end } else { i64::MAX / 4 };
    (-(left.duration - min_duration), high)
}

/// How far a clip starting at a cut may move its start: `(low, high)`.
fn start_range(right: &Clip, min_duration: i64) -> (i64, i64) {
    let low = if right.limited { -right.trim_start } else { -(i64::MAX / 4) };
    (low, right.duration - min_duration)
}

/// Rolls the cut between `left` and `right` (which touch) by `delta`.
pub fn roll(left: &Clip, right: &Clip, delta: i64, min_duration: i64) -> Result<Plan, String> {
    refuse_retimed(&[left, right])?;
    let (a_low, a_high) = end_range(left, min_duration);
    let (b_low, b_high) = start_range(right, min_duration);
    let delta = fit(delta, a_low.max(b_low), a_high.min(b_high));
    Ok(Plan { delta, changes: vec![grow_end(left, delta), move_start(right, delta)] })
}

/// One side of a slide: the clip touching it, or the free time there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Clip(Clip),
    Gap(i64),
}

/// Slides `clip` by `delta` between `before` and `after`.
pub fn slide(clip: &Clip, before: &Side, after: &Side, delta: i64, min_duration: i64) -> Result<Plan, String> {
    let mut clips = vec![clip];
    if let Side::Clip(left) = before {
        clips.push(left);
    }
    if let Side::Clip(right) = after {
        clips.push(right);
    }
    refuse_retimed(&clips)?;
    let (mut low, mut high) = (i64::MIN / 4, i64::MAX / 4);
    match before {
        Side::Clip(left) => {
            let (a, b) = end_range(left, min_duration);
            low = low.max(a);
            high = high.min(b);
        }
        Side::Gap(gap) => low = low.max(-gap.max(&0)),
    }
    match after {
        Side::Clip(right) => {
            let (a, b) = start_range(right, min_duration);
            low = low.max(a);
            high = high.min(b);
        }
        Side::Gap(gap) => high = high.min(*gap.max(&0)),
    }
    let delta = fit(delta, low, high);
    let mut changes = Vec::new();
    if let Side::Clip(left) = before {
        changes.push(grow_end(left, delta));
    }
    changes.push(change(clip, clip.start + delta, clip.duration, clip.trim_start, clip.trim_end));
    if let Side::Clip(right) = after {
        changes.push(move_start(right, delta));
    }
    Ok(Plan { delta, changes })
}

/// How close two clips' ends must be to count as touching (a hundredth of
/// a second).
pub const TOUCH_TICKS: i64 = 1200;

/// A clip's neighbours on its track, for slide and roll: the clip touching
/// its start (or the free time before it, down to 0) and the one touching
/// its end (or the free time after it: to the next clip, unbounded at the
/// end of the track). `others` are the track's other clips.
pub fn sides(clip: &Clip, others: &[Clip]) -> (Side, Side) {
    let end = clip.start + clip.duration;
    let before = others
        .iter()
        .filter(|other| other.id != clip.id && other.start + other.duration <= clip.start + TOUCH_TICKS && other.start < clip.start)
        .max_by_key(|other| other.start + other.duration);
    let after = others
        .iter()
        .filter(|other| other.id != clip.id && other.start >= end - TOUCH_TICKS && other.start > clip.start)
        .min_by_key(|other| other.start);
    let before = match before {
        Some(left) if (clip.start - (left.start + left.duration)).abs() <= TOUCH_TICKS => Side::Clip(left.clone()),
        Some(left) => Side::Gap(clip.start - (left.start + left.duration)),
        None => Side::Gap(clip.start),
    };
    let after = match after {
        Some(right) if (right.start - end).abs() <= TOUCH_TICKS => Side::Clip(right.clone()),
        Some(right) => Side::Gap(right.start - end),
        None => Side::Gap(i64::MAX / 4),
    };
    (before, after)
}

/// The whole edit for a tool on `target` among its track's clips: slip it,
/// slide it, or roll the cut at its end (into the clip after it).
pub fn plan(tool: Tool, target: &Clip, track: &[Clip], delta: i64, min_duration: i64) -> Result<Plan, String> {
    match tool {
        Tool::Slip => slip(target, delta),
        Tool::Slide => {
            let (before, after) = sides(target, track);
            slide(target, &before, &after, delta, min_duration)
        }
        Tool::Roll => match sides(target, track).1 {
            Side::Clip(right) => roll(target, &right, delta, min_duration),
            Side::Gap(_) => Err("Roll moves a cut between two clips that touch; nothing starts where this clip ends".to_owned()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn plans_match_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/trimedit/cases.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let tool = Tool::parse(case["tool"].as_str().unwrap()).unwrap();
            let track: Vec<Clip> = serde_json::from_value(case["track"].clone()).unwrap();
            let target = track.iter().find(|clip| clip.id == case["target"].as_str().unwrap()).unwrap();
            let result = plan(tool, target, &track, case["delta"].as_i64().unwrap(), case["minDuration"].as_i64().unwrap());
            match case.get("error") {
                Some(_) => assert!(result.is_err(), "{name}: {result:?}"),
                None => {
                    let want: Plan = serde_json::from_value(case["plan"].clone()).unwrap();
                    assert_eq!(result.unwrap(), want, "{name}");
                }
            }
        }
    }
}

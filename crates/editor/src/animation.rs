//! Keyframe animation of numeric element parameters.
//!
//! An element's `animations` map a parameter path (`opacity`,
//! `transform.positionX`, `effects.<id>.params.intensity`, …) to a channel
//! of keys whose times are relative to the element's start, with the same
//! JSON shape as the web app's `animations` object. Between keys the value
//! is held, interpolated linearly or follows a cubic Bézier through the
//! keys' curve handles; before the first and after the last key it is held.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::MediaTime;

use crate::params::Params;

/// How a key's value reaches the next key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Segment {
    /// Holds this key's value until the next key.
    Step,
    #[default]
    Linear,
    /// A cubic Bézier in time and value from this key's `right_handle` to
    /// the next key's `left_handle`. Without handles it is a straight line,
    /// exactly as in the web app's graph editor.
    Bezier,
}

/// One control point of a Bézier segment, relative to its key: `dt` ticks
/// along the time axis and `dv` in the parameter's unit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveHandle {
    pub dt: MediaTime,
    pub dv: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub id: String,
    /// Relative to the element's start.
    pub time: MediaTime,
    pub value: f64,
    #[serde(default)]
    pub segment_to_next: Segment,
    /// Control point of the segment arriving from the previous key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_handle: Option<CurveHandle>,
    /// Control point of the segment leaving towards the next key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_handle: Option<CurveHandle>,
    /// The web app's graph-editor tangent mode, kept as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangent_mode: Option<String>,
}

impl Keyframe {
    pub fn new(time: MediaTime, value: f64, segment_to_next: Segment) -> Self {
        Self {
            id: crate::new_id(),
            time,
            value,
            segment_to_next,
            left_handle: None,
            right_handle: None,
            tangent_mode: None,
        }
    }

    /// Flat handles a third of the way to each neighbour: the symmetric
    /// ease the web app's graph editor starts a Bézier segment with.
    pub fn flat_handles(span: MediaTime) -> (CurveHandle, CurveHandle) {
        let third = MediaTime::from_ticks(span.as_ticks() / 3);
        (
            CurveHandle { dt: third, dv: 0.0 },
            CurveHandle { dt: MediaTime::from_ticks(-third.as_ticks()), dv: 0.0 },
        )
    }
}

/// A Bézier segment's four control points in (ticks, value), with the web
/// app's defaults and clamping: a missing handle sits a third of the way
/// along the straight line, and handles never leave the segment's time span.
fn bezier_points(left: &Keyframe, right: &Keyframe) -> [(f64, f64); 4] {
    let span = (right.time - left.time).as_ticks().max(1) as f64;
    let delta = right.value - left.value;
    let (p1, p2) = match (left.right_handle, right.left_handle) {
        (right_handle, left_handle) => {
            let p1 = match right_handle {
                Some(handle) => ((handle.dt.as_ticks() as f64).clamp(0.0, span), handle.dv),
                None => (span / 3.0, delta / 3.0),
            };
            let p2 = match left_handle {
                Some(handle) => ((handle.dt.as_ticks() as f64).clamp(-span, 0.0), handle.dv),
                None => (-span / 3.0, -delta / 3.0),
            };
            (p1, p2)
        }
    };
    let t0 = left.time.as_ticks() as f64;
    let t1 = right.time.as_ticks() as f64;
    [
        (t0, left.value),
        (t0 + p1.0, left.value + p1.1),
        (t1 + p2.0, right.value + p2.1),
        (t1, right.value),
    ]
}

fn cubic(u: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let m = 1.0 - u;
    m * m * m * p0 + 3.0 * m * m * u * p1 + 3.0 * m * u * u * p2 + u * u * u * p3
}

/// The curve parameter at which the segment's time coordinate reaches
/// `time` (bisection, as the web app does it).
fn bezier_progress_for_time(points: &[(f64, f64); 4], time: f64) -> f64 {
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..20 {
        let mid = (lower + upper) / 2.0;
        if cubic(mid, points[0].0, points[1].0, points[2].0, points[3].0) < time {
            lower = mid;
        } else {
            upper = mid;
        }
    }
    (lower + upper) / 2.0
}

/// The value of a Bézier segment at `time` (ticks, within the segment).
fn bezier_value_at(left: &Keyframe, right: &Keyframe, time: f64) -> f64 {
    let points = bezier_points(left, right);
    let u = bezier_progress_for_time(&points, time);
    cubic(u, points[0].1, points[1].1, points[2].1, points[3].1)
}

/// Keys of one parameter, sorted by time with at most one key per time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    #[serde(default)]
    pub keys: Vec<Keyframe>,
}

/// Parameter path → channel.
pub type Animations = BTreeMap<String, Channel>;

/// Keys closer than this to a time count as being at that time.
pub const KEY_SNAP: MediaTime = MediaTime::from_ticks(600); // 5 ms

impl Channel {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The animated value at an element-relative time, or `None` for a
    /// channel without keys.
    pub fn value_at(&self, time: MediaTime) -> Option<f64> {
        let first = self.keys.first()?;
        let last = self.keys.last()?;
        if time <= first.time {
            return Some(first.value);
        }
        if time >= last.time {
            return Some(last.value);
        }
        let next = self.keys.iter().position(|key| key.time > time)?;
        let left = &self.keys[next - 1];
        let right = &self.keys[next];
        let span = (right.time - left.time).as_ticks() as f64;
        let progress = if span <= 0.0 {
            1.0
        } else {
            ((time - left.time).as_ticks() as f64 / span).clamp(0.0, 1.0)
        };
        match left.segment_to_next {
            Segment::Step => Some(left.value),
            Segment::Linear => Some(left.value + (right.value - left.value) * progress),
            Segment::Bezier => Some(bezier_value_at(left, right, time.as_ticks() as f64)),
        }
    }

    /// The key at `time` (within [`KEY_SNAP`]), if any.
    pub fn key_at(&self, time: MediaTime) -> Option<&Keyframe> {
        self.keys
            .iter()
            .find(|key| (key.time - time).as_ticks().abs() <= KEY_SNAP.as_ticks())
    }

    /// Sets the value at `time`, replacing a key already there. Returns the
    /// key's id.
    pub fn upsert(&mut self, time: MediaTime, value: f64, segment: Option<Segment>) -> String {
        if let Some(index) = self
            .keys
            .iter()
            .position(|key| (key.time - time).as_ticks().abs() <= KEY_SNAP.as_ticks())
        {
            let key = &mut self.keys[index];
            key.value = value;
            if let Some(segment) = segment {
                key.segment_to_next = segment;
            }
            return key.id.clone();
        }
        let key = Keyframe::new(time, value, segment.unwrap_or_default());
        let id = key.id.clone();
        self.keys.push(key);
        self.normalize();
        id
    }

    pub fn remove(&mut self, keyframe_id: &str) -> bool {
        let before = self.keys.len();
        self.keys.retain(|key| key.id != keyframe_id);
        self.keys.len() != before
    }

    /// Moves every key by `delta` (keys keep following the same source
    /// frames when the element's start is trimmed).
    pub fn shift(&mut self, delta: MediaTime) {
        for key in &mut self.keys {
            key.time = key.time + delta;
        }
    }

    /// Splits at an element-relative time into the channel of the left
    /// part and the channel of the right part (whose times start again at
    /// zero). Both get a key at the boundary so the motion continues
    /// unchanged across the cut.
    pub fn split(&self, at: MediaTime) -> (Channel, Channel) {
        let mut left = Channel::default();
        let mut right = Channel::default();
        if self.keys.is_empty() {
            return (left, right);
        }
        let boundary = self.value_at(at).unwrap_or(0.0);
        let segment_at_boundary = self
            .keys
            .iter()
            .rev()
            .find(|key| key.time <= at)
            .map(|key| key.segment_to_next)
            .unwrap_or_default();
        let any_before = self.keys.iter().any(|key| key.time < at);
        let any_after = self.keys.iter().any(|key| key.time > at);
        for key in &self.keys {
            if key.time < at {
                left.keys.push(key.clone());
            } else if key.time > at {
                let mut key = key.clone();
                key.time = key.time - at;
                key.id = crate::new_id();
                right.keys.push(key);
            }
        }
        let mut boundary_left = Keyframe::new(at, boundary, segment_at_boundary);
        let mut boundary_right = Keyframe::new(MediaTime::ZERO, boundary, segment_at_boundary);
        // Cutting through a Bézier segment: split the curve so both halves
        // keep its shape (de Casteljau at the boundary's curve parameter).
        let cut = self
            .keys
            .iter()
            .position(|key| key.time > at)
            .filter(|&next| next > 0 && self.keys[next - 1].time < at);
        if let Some(next) = cut
            && self.keys[next - 1].segment_to_next == Segment::Bezier
        {
            let points = bezier_points(&self.keys[next - 1], &self.keys[next]);
            let u = bezier_progress_for_time(&points, at.as_ticks() as f64);
            let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
            let [p0, p1, p2, p3] = points;
            let (q0, q1, q2) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
            let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
            let s = lerp(r0, r1);
            let handle = |from: (f64, f64), to: (f64, f64)| CurveHandle {
                dt: MediaTime::from_ticks((to.0 - from.0).round() as i64),
                dv: to.1 - from.1,
            };
            if let Some(key) = left.keys.last_mut() {
                key.right_handle = Some(handle(p0, q0));
            }
            boundary_left.left_handle = Some(handle(s, r0));
            boundary_right.right_handle = Some(handle(s, r1));
            if let Some(key) = right.keys.first_mut() {
                key.left_handle = Some(handle(p3, q2));
            }
        }
        if any_after || self.key_at(at).is_some() {
            left.keys.push(boundary_left);
        }
        if any_before || self.key_at(at).is_some() {
            right.keys.insert(0, boundary_right);
        }
        left.normalize();
        right.normalize();
        (left, right)
    }

    /// Sorts the keys and drops later duplicates at the same time.
    pub fn normalize(&mut self) {
        self.keys.sort_by_key(|key| key.time);
        self.keys.dedup_by(|later, earlier| later.time == earlier.time);
    }
}

/// Paths under `effects.` and `masks.` belong to those objects rather
/// than to the element's own parameters.
pub fn is_param_path(path: &str) -> bool {
    !path.starts_with("effects.") && !path.starts_with("masks.")
}

/// `params` with every animated parameter replaced by its value at an
/// element-relative time.
pub fn resolve_params(params: &Params, animations: &Animations, time: MediaTime) -> Params {
    let mut resolved = params.clone();
    for (path, channel) in animations {
        if !is_param_path(path) {
            continue;
        }
        if let Some(value) = channel.value_at(time) {
            resolved.set(path.clone(), value);
        }
    }
    resolved
}

/// `params` of the effect or mask `object_id` with its animated parameters
/// (`<prefix>.<object_id>.params.<key>`) resolved at an element-relative
/// time.
pub fn resolve_object_params(
    params: &Params,
    animations: &Animations,
    prefix: &str,
    object_id: &str,
    time: MediaTime,
) -> Params {
    let mut resolved = params.clone();
    let path_prefix = format!("{prefix}.{object_id}.params.");
    for (path, channel) in animations {
        if let Some(key) = path.strip_prefix(&path_prefix)
            && let Some(value) = channel.value_at(time)
        {
            resolved.set(key.to_owned(), value);
        }
    }
    resolved
}

/// Every channel split at an element-relative time; see [`Channel::split`].
pub fn split_animations(animations: &Animations, at: MediaTime) -> (Animations, Animations) {
    let mut left = Animations::new();
    let mut right = Animations::new();
    for (path, channel) in animations {
        let (left_channel, right_channel) = channel.split(at);
        if !left_channel.is_empty() {
            left.insert(path.clone(), left_channel);
        }
        if !right_channel.is_empty() {
            right.insert(path.clone(), right_channel);
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(value: i64) -> MediaTime {
        MediaTime::from_ticks(value)
    }

    fn channel(keys: &[(i64, f64, Segment)]) -> Channel {
        let mut channel = Channel::default();
        for (time, value, segment) in keys {
            channel.upsert(ticks(*time), *value, Some(*segment));
        }
        channel
    }

    #[test]
    fn holds_outside_and_interpolates_between_keys() {
        let channel = channel(&[(1000, 0.0, Segment::Linear), (3000, 10.0, Segment::Step), (5000, 20.0, Segment::Linear)]);
        assert_eq!(channel.value_at(ticks(0)), Some(0.0));
        assert_eq!(channel.value_at(ticks(2000)), Some(5.0));
        assert_eq!(channel.value_at(ticks(3000)), Some(10.0));
        assert_eq!(channel.value_at(ticks(4999)), Some(10.0), "step holds");
        assert_eq!(channel.value_at(ticks(5000)), Some(20.0));
        assert_eq!(channel.value_at(ticks(9000)), Some(20.0));
        assert_eq!(Channel::default().value_at(ticks(0)), None);
    }

    #[test]
    fn bezier_without_handles_is_a_straight_line_like_the_web() {
        let channel = channel(&[(0, 0.0, Segment::Bezier), (4000, 8.0, Segment::Linear)]);
        assert!((channel.value_at(ticks(1000)).unwrap() - 2.0).abs() < 1e-4);
        assert!((channel.value_at(ticks(3000)).unwrap() - 6.0).abs() < 1e-4);
    }

    #[test]
    fn bezier_follows_curve_handles() {
        // Flat handles a third of the way: a symmetric ease (smoothstep).
        let mut channel = channel(&[(0, 0.0, Segment::Bezier), (4000, 8.0, Segment::Linear)]);
        let (right, left) = Keyframe::flat_handles(ticks(4000));
        channel.keys[0].right_handle = Some(right);
        channel.keys[1].left_handle = Some(left);
        let quarter = channel.value_at(ticks(1000)).unwrap();
        let half = channel.value_at(ticks(2000)).unwrap();
        let three_quarters = channel.value_at(ticks(3000)).unwrap();
        assert!((half - 4.0).abs() < 1e-4, "{half}");
        assert!((quarter - 1.25).abs() < 1e-3, "smoothstep at a quarter: {quarter}");
        assert!((quarter + three_quarters - 8.0).abs() < 1e-4);

        // Only an ease-out handle: fast at first, then slowing down.
        channel.keys[0].right_handle = None;
        let quarter = channel.value_at(ticks(1000)).unwrap();
        assert!(quarter > 2.0 && quarter < 8.0, "{quarter}");
        assert!(channel.value_at(ticks(3999)).unwrap() < 8.0);

        // Handles never leave the segment: an overlong one is clamped.
        channel.keys[0].right_handle = Some(CurveHandle { dt: ticks(40_000), dv: 0.0 });
        let value = channel.value_at(ticks(3000)).unwrap();
        assert!(value.is_finite() && (0.0..=8.0).contains(&value), "{value}");
    }

    #[test]
    fn handles_round_trip_as_json_and_survive_a_split() {
        let mut channel = channel(&[(0, 0.0, Segment::Bezier), (4000, 8.0, Segment::Linear)]);
        channel.keys[0].right_handle = Some(CurveHandle { dt: ticks(2000), dv: 0.0 });
        channel.keys[1].left_handle = Some(CurveHandle { dt: ticks(-100), dv: -3.0 });
        channel.keys[1].tangent_mode = Some("broken".to_owned());
        let json = serde_json::to_string(&channel).unwrap();
        assert!(json.contains("\"rightHandle\":{\"dt\":2000,\"dv\":0.0}"), "{json}");
        assert!(json.contains("\"tangentMode\":\"broken\""), "{json}");
        let back: Channel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, channel);

        let (left, right) = channel.split(ticks(1500));
        for time in [0, 500, 1000, 1499] {
            let original = channel.value_at(ticks(time)).unwrap();
            let split = left.value_at(ticks(time)).unwrap();
            assert!((original - split).abs() < 1e-3, "left at {time}: {original} vs {split}");
        }
        for time in [1501, 2000, 3000, 3999] {
            let original = channel.value_at(ticks(time)).unwrap();
            let split = right.value_at(ticks(time - 1500)).unwrap();
            assert!((original - split).abs() < 1e-3, "right at {time}: {original} vs {split}");
        }
    }

    #[test]
    fn upsert_replaces_keys_at_the_same_time_and_sorts() {
        let mut channel = Channel::default();
        let later = channel.upsert(ticks(5000), 1.0, None);
        let earlier = channel.upsert(ticks(1000), 2.0, None);
        let same = channel.upsert(ticks(5000) + ticks(100), 3.0, None);
        assert_eq!(same, later, "within the snap distance the key is reused");
        assert_eq!(channel.keys.len(), 2);
        assert_eq!(channel.keys[0].id, earlier);
        assert_eq!(channel.keys[1].value, 3.0);
        assert!(channel.remove(&earlier));
        assert!(!channel.remove(&earlier));
        assert_eq!(channel.keys.len(), 1);
    }

    #[test]
    fn split_keeps_the_motion_continuous() {
        let channel = channel(&[(0, 0.0, Segment::Linear), (4000, 8.0, Segment::Linear)]);
        let (left, right) = channel.split(ticks(1000));
        assert_eq!(left.keys.len(), 2);
        assert_eq!(left.keys[1].time, ticks(1000));
        assert_eq!(left.keys[1].value, 2.0);
        assert_eq!(right.keys.len(), 2);
        assert_eq!(right.keys[0].time, ticks(0));
        assert_eq!(right.keys[0].value, 2.0);
        assert_eq!(right.keys[1].time, ticks(3000));
        assert_eq!(right.value_at(ticks(1500)), Some(5.0));

        // A split after every key leaves the right part with a single held key.
        let (left, right) = channel.split(ticks(6000));
        assert_eq!(left.keys.len(), 2, "no boundary key needed with nothing after");
        assert_eq!(right.keys.len(), 1);
        assert_eq!(right.keys[0].value, 8.0);
    }

    #[test]
    fn resolves_element_and_object_params() {
        let mut params = Params::new();
        params.set("opacity", 1.0).set("content", "Hi");
        let mut animations = Animations::new();
        animations.insert("opacity".to_owned(), channel(&[(0, 0.0, Segment::Linear), (2000, 1.0, Segment::Linear)]));
        animations.insert(
            "effects.fx1.params.intensity".to_owned(),
            channel(&[(0, 10.0, Segment::Linear), (2000, 30.0, Segment::Linear)]),
        );
        let resolved = resolve_params(&params, &animations, ticks(1000));
        assert_eq!(resolved.number("opacity", 9.0), 0.5);
        assert_eq!(resolved.text("content", ""), "Hi");
        assert!(resolved.get("effects.fx1.params.intensity").is_none());

        let mut effect = Params::new();
        effect.set("intensity", 15.0);
        let resolved = resolve_object_params(&effect, &animations, "effects", "fx1", ticks(1000));
        assert_eq!(resolved.number("intensity", 0.0), 20.0);
        let other = resolve_object_params(&effect, &animations, "effects", "fx2", ticks(1000));
        assert_eq!(other.number("intensity", 0.0), 15.0);
    }

    #[test]
    fn json_matches_the_web_shape() {
        let channel = channel(&[(0, 0.0, Segment::Bezier)]);
        let json = serde_json::to_string(&channel).unwrap();
        assert!(json.contains("\"segmentToNext\":\"bezier\""), "{json}");
        let back: Channel = serde_json::from_str(
            r#"{"keys":[{"id":"k","time":120000,"value":2,"segmentToNext":"step","tangentMode":"auto"}]}"#,
        )
        .unwrap();
        assert_eq!(back.keys[0].segment_to_next, Segment::Step);
        assert_eq!(back.keys[0].time, ticks(120000));
    }
}

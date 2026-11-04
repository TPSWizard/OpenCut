//! Keyframe animation of numeric element parameters.
//!
//! An element's `animations` map a parameter path (`opacity`,
//! `transform.positionX`, `effects.<id>.params.intensity`, …) to a channel
//! of keys whose times are relative to the element's start, with the same
//! JSON shape as the web app's `animations` object. Between keys the value
//! is held, interpolated linearly or follows a cubic Bézier through the
//! keys' curve handles; before the first and after the last key it is held.
//!
//! A colour (`color`, `background.color`) is animated as the web app does
//! it: four scalar channels `r`, `g`, `b`, `a` in linear light, blended
//! separately and formatted back to a hex colour.

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
/// A colour channel has no keys of its own but `components` (`r`, `g`,
/// `b`, `a`), each a scalar channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    pub keys: Vec<Keyframe>,
    pub components: BTreeMap<String, Channel>,
    /// The web app's extrapolation setting, kept as written.
    pub extrapolation: Option<serde_json::Value>,
}

/// The JSON shapes: `{"keys": [...]}` for a scalar, a map of component
/// channels for a colour. A map with a single `value` entry is the web
/// app's wrapped scalar and reads as one.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ChannelRepr {
    Scalar {
        keys: Vec<Keyframe>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extrapolation: Option<serde_json::Value>,
    },
    Composite(BTreeMap<String, Channel>),
}

impl Serialize for Channel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.components.is_empty() {
            ChannelRepr::Scalar {
                keys: self.keys.clone(),
                extrapolation: self.extrapolation.clone(),
            }
            .serialize(serializer)
        } else {
            ChannelRepr::Composite(self.components.clone()).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Channel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match ChannelRepr::deserialize(deserializer)? {
            ChannelRepr::Scalar { keys, extrapolation } => Self {
                keys,
                components: BTreeMap::new(),
                extrapolation,
            },
            ChannelRepr::Composite(mut components) => {
                if components.len() == 1 && components.contains_key("value") {
                    components.remove("value").expect("checked")
                } else {
                    Self {
                        keys: Vec::new(),
                        components,
                        extrapolation: None,
                    }
                }
            }
        })
    }
}

const COLOR_COMPONENTS: [&str; 4] = ["r", "g", "b", "a"];

fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// `#rrggbb`, or `#rrggbbaa` when the alpha is below one, from linear-light
/// components, as the web app formats an animated colour.
pub fn format_linear_rgba([r, g, b, a]: [f64; 4]) -> String {
    let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b) = (byte(linear_to_srgb(r)), byte(linear_to_srgb(g)), byte(linear_to_srgb(b)));
    if a < 1.0 {
        format!("#{r:02x}{g:02x}{b:02x}{:02x}", byte(a))
    } else {
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

/// A hex colour as linear-light components, the space colour keys live in.
pub fn parse_color_to_linear_rgba(color: &str) -> Option<[f64; 4]> {
    let [r, g, b, a] = crate::frame::parse_hex_color(color)?;
    Some([
        srgb_to_linear(r as f64),
        srgb_to_linear(g as f64),
        srgb_to_linear(b as f64),
        (a as f64).clamp(0.0, 1.0),
    ])
}

/// Parameter path → channel.
pub type Animations = BTreeMap<String, Channel>;

/// Keys closer than this to a time count as being at that time.
pub const KEY_SNAP: MediaTime = MediaTime::from_ticks(600); // 5 ms

impl Channel {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.components.values().all(Channel::is_empty)
    }

    /// Whether this is a colour: `r`, `g`, `b` and `a` component channels.
    pub fn is_color(&self) -> bool {
        COLOR_COMPONENTS.iter().all(|name| self.components.contains_key(*name))
    }

    /// Every key, including those of a colour's components.
    pub fn all_keys(&self) -> impl Iterator<Item = &Keyframe> {
        self.keys
            .iter()
            .chain(self.components.values().flat_map(|component| component.keys.iter()))
    }

    /// The animated colour at an element-relative time, or `None` when this
    /// is not a colour channel (or a component has no keys).
    pub fn color_at(&self, time: MediaTime) -> Option<String> {
        if !self.is_color() {
            return None;
        }
        let mut rgba = [0.0; 4];
        for (index, name) in COLOR_COMPONENTS.iter().enumerate() {
            rgba[index] = self.components.get(*name)?.value_at(time)?;
        }
        Some(format_linear_rgba(rgba))
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

    /// The key at `time` (within [`KEY_SNAP`]), if any. For a colour this
    /// is the key of its first component, which every component shares.
    pub fn key_at(&self, time: MediaTime) -> Option<&Keyframe> {
        let near = |key: &&Keyframe| (key.time - time).as_ticks().abs() <= KEY_SNAP.as_ticks();
        match self.keys.iter().find(near) {
            Some(key) => Some(key),
            None => self.components.get(COLOR_COMPONENTS[0])?.keys.iter().find(near),
        }
    }

    /// Sets a colour key at `time` from linear-light components: one key id
    /// shared by the `r`, `g`, `b` and `a` channels, as the web app writes
    /// it, replacing a key already there. Returns the key's id. Fails on a
    /// channel that holds scalar keys.
    pub fn upsert_color(&mut self, time: MediaTime, rgba: [f64; 4]) -> Result<String, String> {
        if !self.keys.is_empty() {
            return Err("a number channel cannot hold a colour key".to_owned());
        }
        let existing = self
            .components
            .get(COLOR_COMPONENTS[0])
            .and_then(|channel| channel.key_at(time))
            .map(|key| (key.id.clone(), key.time));
        let (id, time) = existing.unwrap_or_else(|| (crate::new_id(), time));
        for (name, value) in COLOR_COMPONENTS.iter().zip(rgba) {
            let component = self.components.entry((*name).to_owned()).or_default();
            if let Some(key) = component.keys.iter_mut().find(|key| key.id == id) {
                key.value = value;
            } else if let Some(key) = component.keys.iter_mut().find(|key| (key.time - time).as_ticks().abs() <= KEY_SNAP.as_ticks()) {
                key.value = value;
            } else {
                let mut key = Keyframe::new(time, value, Segment::Linear);
                key.id = id.clone();
                component.keys.push(key);
                component.normalize();
            }
        }
        Ok(id)
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
        let mut removed = self.keys.len() != before;
        for component in self.components.values_mut() {
            removed |= component.remove(keyframe_id);
        }
        removed
    }

    /// Moves every key by `delta` (keys keep following the same source
    /// frames when the element's start is trimmed).
    pub fn shift(&mut self, delta: MediaTime) {
        for key in &mut self.keys {
            key.time = key.time + delta;
        }
        for component in self.components.values_mut() {
            component.shift(delta);
        }
    }

    /// Splits at an element-relative time into the channel of the left
    /// part and the channel of the right part (whose times start again at
    /// zero). Both get a key at the boundary so the motion continues
    /// unchanged across the cut.
    pub fn split(&self, at: MediaTime) -> (Channel, Channel) {
        // The two boundary keys get ids shared by every component of a
        // colour, and keys keep their ids on both sides (they end up in
        // different elements), as the web app's split does.
        self.split_with(at, &crate::new_id(), &crate::new_id())
    }

    fn split_with(&self, at: MediaTime, left_boundary_id: &str, right_boundary_id: &str) -> (Channel, Channel) {
        let mut left = Channel::default();
        let mut right = Channel::default();
        if !self.components.is_empty() {
            for (name, component) in &self.components {
                let (left_part, right_part) = component.split_with(at, left_boundary_id, right_boundary_id);
                left.components.insert(name.clone(), left_part);
                right.components.insert(name.clone(), right_part);
            }
            return (left, right);
        }
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
                right.keys.push(key);
            }
        }
        let mut boundary_left = Keyframe::new(at, boundary, segment_at_boundary);
        boundary_left.id = left_boundary_id.to_owned();
        let mut boundary_right = Keyframe::new(MediaTime::ZERO, boundary, segment_at_boundary);
        boundary_right.id = right_boundary_id.to_owned();
        // A key right on the cut is split in two that keep its handles.
        if let Some(key) = self.keys.iter().find(|key| key.time == at) {
            boundary_left.left_handle = key.left_handle;
            boundary_right.right_handle = key.right_handle;
            boundary_right.segment_to_next = key.segment_to_next;
            boundary_right.tangent_mode = key.tangent_mode.clone();
        }
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
        for component in self.components.values_mut() {
            component.normalize();
        }
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
        if let Some(color) = channel.color_at(time) {
            resolved.set(path.clone(), color);
        } else if let Some(value) = channel.value_at(time) {
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
        if let Some(key) = path.strip_prefix(&path_prefix) {
            if let Some(color) = channel.color_at(time) {
                resolved.set(key.to_owned(), color);
            } else if let Some(value) = channel.value_at(time) {
                resolved.set(key.to_owned(), value);
            }
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
    fn color_channels_blend_in_linear_light() {
        let ramp = |name: &str, from: f64, to: f64| {
            format!(
                r#""{name}":{{"keys":[{{"id":"{name}0","time":0,"value":{from},"segmentToNext":"linear","tangentMode":"flat"}},{{"id":"{name}1","time":2000,"value":{to},"segmentToNext":"linear","tangentMode":"flat"}}]}}"#
            )
        };
        let json = format!(
            "{{{},{},{},{}}}",
            ramp("r", 0.0, 1.0),
            ramp("g", 0.0, 1.0),
            ramp("b", 0.0, 1.0),
            r#""a":{"keys":[{"id":"a0","time":0,"value":1}]}"#
        );
        let channel: Channel = serde_json::from_str(&json).unwrap();
        assert!(channel.is_color());
        assert!(!channel.is_empty());
        assert_eq!(channel.all_keys().count(), 7);
        assert_eq!(channel.value_at(ticks(0)), None, "not a scalar");
        assert_eq!(channel.color_at(ticks(0)).as_deref(), Some("#000000"));
        assert_eq!(channel.color_at(ticks(2000)).as_deref(), Some("#ffffff"));
        // Linear 0.5 is sRGB 0.735: the midpoint is lighter than #808080.
        assert_eq!(channel.color_at(ticks(1000)).as_deref(), Some("#bcbcbc"));

        let back: Channel = serde_json::from_str(&serde_json::to_string(&channel).unwrap()).unwrap();
        assert_eq!(back, channel);
        assert!(serde_json::to_string(&channel).unwrap().starts_with(r#"{"a":{"keys""#));

        let mut animations = Animations::new();
        animations.insert("color".to_owned(), channel.clone());
        let resolved = resolve_params(&Params::new(), &animations, ticks(1000));
        assert_eq!(resolved.text("color", ""), "#bcbcbc");

        // A cut keeps every component; alpha below one adds two digits.
        let (left, right) = channel.split(ticks(1000));
        assert_eq!(left.color_at(ticks(1000)).as_deref(), Some("#bcbcbc"));
        assert_eq!(right.color_at(ticks(0)).as_deref(), Some("#bcbcbc"));
        assert_eq!(right.color_at(ticks(1000)).as_deref(), Some("#ffffff"));
        let mut faded = channel.clone();
        faded.components.get_mut("a").unwrap().keys[0].value = 0.5;
        assert_eq!(faded.color_at(ticks(2000)).as_deref(), Some("#ffffff80"));
        assert!(faded.remove("a0"));
        assert!(faded.color_at(ticks(0)).is_none(), "a component without keys has no colour");

        // Both sides of a cut keep one id per colour key, so a key can
        // still be found and removed as a whole.
        let (left, right) = channel.split(ticks(1000));
        let boundary = left.key_at(ticks(1000)).unwrap().id.clone();
        // (`a` is constant, so its left side needs no boundary key.)
        assert!(left.components.values().filter_map(|component| component.key_at(ticks(1000))).all(|key| key.id == boundary));
        assert_eq!(left.components.values().filter(|component| component.key_at(ticks(1000)).is_some()).count(), 3);
        let right_boundary = right.key_at(ticks(0)).unwrap().id.clone();
        assert!(right.components.values().all(|component| component.keys[0].id == right_boundary));
        assert_ne!(boundary, right_boundary);
        let mut left = left;
        assert!(left.remove(&boundary));
        assert!(left.components.values().all(|component| component.keys.len() == 1));
        assert_eq!(right.components["r"].keys[1].id, "r1", "keys keep their ids across the cut");

        // The web app's wrapped scalar reads as a plain channel.
        let wrapped: Channel =
            serde_json::from_str(r#"{"value":{"keys":[{"id":"k","time":0,"value":3}]}}"#).unwrap();
        assert!(wrapped.components.is_empty());
        assert_eq!(wrapped.value_at(ticks(0)), Some(3.0));
        assert_eq!(serde_json::to_string(&wrapped).unwrap(), r#"{"keys":[{"id":"k","time":0,"value":3.0,"segmentToNext":"linear"}]}"#);

        assert_eq!(parse_color_to_linear_rgba("#bcbcbc").map(|c| (c[0] * 100.0).round()), Some(50.0));
        assert_eq!(format_linear_rgba(parse_color_to_linear_rgba("#12345678").unwrap()), "#12345678");
    }

    #[test]
    fn color_keys_are_set_as_one_id_across_the_components() {
        let mut channel = Channel::default();
        let first = channel.upsert_color(ticks(0), parse_color_to_linear_rgba("#000000").unwrap()).unwrap();
        let second = channel.upsert_color(ticks(2000), parse_color_to_linear_rgba("#ffffff").unwrap()).unwrap();
        assert!(channel.is_color());
        assert_eq!(channel.all_keys().count(), 8);
        assert!(channel.components.values().all(|component| component.keys.iter().any(|key| key.id == first)));
        assert_eq!(channel.key_at(ticks(2000)).map(|key| key.id.as_str()), Some(second.as_str()));
        assert_eq!(channel.color_at(ticks(1000)).as_deref(), Some("#bcbcbc"));
        // Replacing the key at a time keeps its id; removing it empties nothing else.
        let again = channel.upsert_color(ticks(2000) + ticks(100), parse_color_to_linear_rgba("#ff000080").unwrap()).unwrap();
        assert_eq!(again, second);
        assert_eq!(channel.color_at(ticks(2000)).as_deref(), Some("#ff000080"));
        assert!(channel.remove(&first));
        assert_eq!(channel.all_keys().count(), 4);
        assert!(!channel.is_empty());
        assert!(channel.remove(&second));
        assert!(channel.is_empty(), "components without keys count as empty");
        let mut scalar = Channel::default();
        scalar.upsert(ticks(0), 1.0, None);
        assert!(scalar.upsert_color(ticks(0), [0.0; 4]).is_err());
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

        // Cutting exactly on the second key keeps its left handle on the
        // left part, so the curve before it is unchanged.
        let (left, _) = channel.split(ticks(4000));
        assert_eq!(left.keys.len(), 2);
        assert_eq!(left.keys[1].left_handle, channel.keys[1].left_handle);
        for time in [500, 2000, 3500] {
            let original = channel.value_at(ticks(time)).unwrap();
            let split = left.value_at(ticks(time)).unwrap();
            assert!((original - split).abs() < 1e-9, "at {time}: {original} vs {split}");
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

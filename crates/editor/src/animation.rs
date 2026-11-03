//! Keyframe animation of numeric element parameters.
//!
//! An element's `animations` map a parameter path (`opacity`,
//! `transform.positionX`, `effects.<id>.params.intensity`, …) to a channel
//! of keys whose times are relative to the element's start, with the same
//! JSON shape as the web app's `animations` object. Between keys the value
//! is held, interpolated linearly or eased; before the first and after the
//! last key it is held.

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
    /// Eases in and out. The web app's custom curve handles are not read;
    /// a symmetric ease is used instead.
    Bezier,
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
        let progress = match left.segment_to_next {
            Segment::Step => return Some(left.value),
            Segment::Linear => progress,
            Segment::Bezier => progress * progress * (3.0 - 2.0 * progress),
        };
        Some(left.value + (right.value - left.value) * progress)
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
        let id = crate::new_id();
        self.keys.push(Keyframe {
            id: id.clone(),
            time,
            value,
            segment_to_next: segment.unwrap_or_default(),
        });
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
        if any_after || self.key_at(at).is_some() {
            left.keys.push(Keyframe {
                id: crate::new_id(),
                time: at,
                value: boundary,
                segment_to_next: segment_at_boundary,
            });
        }
        if any_before || self.key_at(at).is_some() {
            right.keys.insert(
                0,
                Keyframe {
                    id: crate::new_id(),
                    time: MediaTime::ZERO,
                    value: boundary,
                    segment_to_next: segment_at_boundary,
                },
            );
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
    fn bezier_eases_symmetrically() {
        let channel = channel(&[(0, 0.0, Segment::Bezier), (4000, 8.0, Segment::Linear)]);
        let quarter = channel.value_at(ticks(1000)).unwrap();
        let half = channel.value_at(ticks(2000)).unwrap();
        let three_quarters = channel.value_at(ticks(3000)).unwrap();
        assert!((half - 4.0).abs() < 1e-9);
        assert!(quarter < 2.0 && three_quarters > 6.0);
        assert!((quarter + three_quarters - 8.0).abs() < 1e-9);
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

//! Speed changes: a constant rate, a ramp (the rate as a piecewise-linear
//! function of the clip's own time, keys in seconds, held flat outside
//! them) whose integral is the source time, and playing backwards. The
//! web app's `src/retime/resolve.ts` is the same arithmetic.

use serde::{Deserialize, Serialize};

pub const MIN_RETIME_RATE: f64 = 0.01;
pub const MAX_RETIME_RATE: f64 = 5.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RampKey {
    /// Seconds from the clip's start.
    pub time: f64,
    pub rate: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Retime {
    #[serde(default = "one")]
    pub rate: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub maintain_pitch: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ramp: Vec<RampKey>,
}

fn one() -> f64 {
    1.0
}

impl Default for Retime {
    fn default() -> Self {
        Self { rate: 1.0, maintain_pitch: false, reverse: false, ramp: Vec::new() }
    }
}

pub fn clamp_retime_rate(rate: f64) -> f64 {
    if !rate.is_finite() || rate <= 0.0 { 1.0 } else { rate.clamp(MIN_RETIME_RATE, MAX_RETIME_RATE) }
}

impl Retime {
    /// A plain speed change.
    pub fn constant(rate: f64) -> Self {
        Self { rate: clamp_retime_rate(rate), ..Default::default() }
    }

    /// The ramp's keys sorted by time with safe rates, or none without a usable ramp.
    fn keys(&self) -> Option<Vec<RampKey>> {
        let mut keys: Vec<RampKey> = self
            .ramp
            .iter()
            .filter(|key| key.time.is_finite() && key.time >= 0.0)
            .map(|key| RampKey { time: key.time, rate: clamp_retime_rate(key.rate) })
            .collect();
        if keys.is_empty() {
            return None;
        }
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        Some(keys)
    }

    /// Whether the source time is not a plain multiple of the clip time.
    pub fn is_variable(&self) -> bool {
        self.reverse || self.keys().is_some()
    }

    /// Whether this changes playback at all.
    pub fn is_identity(&self) -> bool {
        !self.reverse && self.keys().is_none() && (clamp_retime_rate(self.rate) - 1.0).abs() < 1e-9
    }

    /// The playback rate at `seconds` of clip time.
    pub fn rate_at(&self, seconds: f64) -> f64 {
        let Some(keys) = self.keys() else { return clamp_retime_rate(self.rate) };
        if seconds <= keys[0].time {
            return keys[0].rate;
        }
        let last = &keys[keys.len() - 1];
        if seconds >= last.time {
            return last.rate;
        }
        for pair in keys.windows(2) {
            let (from, to) = (&pair[0], &pair[1]);
            if seconds <= to.time {
                let span = to.time - from.time;
                let t = if span > 0.0 { (seconds - from.time) / span } else { 1.0 };
                return from.rate + (to.rate - from.rate) * t;
            }
        }
        last.rate
    }

    /// Source seconds consumed by the first `seconds` of clip time.
    pub fn forward(&self, seconds: f64) -> f64 {
        if seconds <= 0.0 {
            return 0.0;
        }
        let Some(keys) = self.keys() else { return seconds * clamp_retime_rate(self.rate) };
        let first = &keys[0];
        if seconds <= first.time {
            return seconds * first.rate;
        }
        let mut total = first.time * first.rate;
        let mut cursor = first.time;
        for pair in keys.windows(2) {
            let (from, to) = (&pair[0], &pair[1]);
            if to.time <= cursor {
                continue;
            }
            let end = seconds.min(to.time);
            if end > cursor {
                let span = to.time - from.time;
                let rate_at = |t: f64| if span > 0.0 { from.rate + (to.rate - from.rate) * (t - from.time) / span } else { to.rate };
                total += (rate_at(cursor) + rate_at(end)) / 2.0 * (end - cursor);
                cursor = end;
            }
            if seconds <= to.time {
                return total;
            }
        }
        total + (seconds - cursor) * keys[keys.len() - 1].rate
    }

    /// The clip time whose forward map reaches `source` seconds.
    pub fn inverse(&self, source: f64) -> f64 {
        if source <= 0.0 {
            return 0.0;
        }
        let Some(keys) = self.keys() else { return source / clamp_retime_rate(self.rate) };
        let slowest = keys.iter().map(|key| key.rate).fold(f64::INFINITY, f64::min);
        let (mut low, mut high) = (0.0, (source / slowest).max(1.0));
        while self.forward(high) < source && high < 1e7 {
            high *= 2.0;
        }
        for _ in 0..60 {
            let mid = (low + high) / 2.0;
            if self.forward(mid) < source { low = mid } else { high = mid }
        }
        (low + high) / 2.0
    }

    /// Source seconds (from the trim start) at `seconds` of clip time, for a
    /// clip `duration` seconds long (needed to play backwards).
    pub fn source_offset(&self, seconds: f64, duration: f64) -> f64 {
        let forward = self.forward(seconds);
        if self.reverse && duration > 0.0 {
            (self.forward(duration) - forward).max(0.0)
        } else {
            forward
        }
    }

    /// Timeline seconds that play `source_span` seconds of source.
    pub fn duration_for_source_span(&self, source_span: f64) -> f64 {
        if source_span <= 0.0 { 0.0 } else { self.inverse(source_span) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(keys: &[(f64, f64)]) -> Retime {
        Retime { ramp: keys.iter().map(|(time, rate)| RampKey { time: *time, rate: *rate }).collect(), ..Default::default() }
    }

    #[test]
    fn constant_rates_scale_and_clamp() {
        assert_eq!(Retime::constant(2.0).forward(5.0), 10.0);
        assert_eq!(Retime::constant(0.0).forward(5.0), 5.0);
        assert_eq!(Retime::constant(100.0).forward(5.0), 25.0);
        assert!((Retime::constant(0.5).duration_for_source_span(10.0) - 20.0).abs() < 1e-9);
        assert!(Retime::default().is_identity());
        assert!(!Retime::constant(2.0).is_identity());
    }

    #[test]
    fn ramps_integrate_the_rate_and_invert() {
        // 1x for the first second, rising to 3x by the third, then flat.
        let retime = ramp(&[(1.0, 1.0), (3.0, 3.0)]);
        assert_eq!(retime.rate_at(0.0), 1.0);
        assert_eq!(retime.rate_at(2.0), 2.0);
        assert_eq!(retime.rate_at(10.0), 3.0);
        assert!((retime.forward(1.0) - 1.0).abs() < 1e-9);
        assert!((retime.forward(3.0) - 5.0).abs() < 1e-9, "1 + trapezoid (1+3)/2*2 = 5");
        assert!((retime.forward(4.0) - 8.0).abs() < 1e-9);
        assert!((retime.inverse(8.0) - 4.0).abs() < 1e-6);
        assert!((retime.inverse(3.0) - 2.2360679).abs() < 1e-5, "{}", retime.inverse(3.0));
        assert!(retime.is_variable());
        let json = serde_json::to_string(&retime).unwrap();
        assert!(json.contains("\"ramp\":[{\"time\":1.0,\"rate\":1.0}") && !json.contains("maintainPitch"), "{json}");
        let back: Retime = serde_json::from_str(r#"{"rate":2,"maintainPitch":true,"reverse":true}"#).unwrap();
        assert!(back.reverse && back.maintain_pitch && back.ramp.is_empty());
    }

    #[test]
    fn reverse_plays_the_span_backwards() {
        let retime = Retime { reverse: true, ..Retime::constant(2.0) };
        assert_eq!(retime.source_offset(0.0, 5.0), 10.0);
        assert_eq!(retime.source_offset(5.0, 5.0), 0.0);
        assert_eq!(retime.source_offset(1.0, 5.0), 8.0);
        assert_eq!(retime.source_offset(1.0, 0.0), 2.0, "without a duration it plays forwards");
    }
}

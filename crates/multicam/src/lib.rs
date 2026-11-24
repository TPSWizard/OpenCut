//! Multicam: several camera angles of one scene, lined up by their sound,
//! and a cut list that says which angle shows when.
//!
//! Syncing: each angle's mono audio becomes an onset envelope ([`onsets`]:
//! log loudness every [`ENVELOPE_RATE`]th of a second, rises only), and the
//! lag at which another angle's onsets best match the reference's is found
//! by FFT cross-correlation with a parabolic sub-step peak ([`sync_lag`]).
//!
//! Switching: the group keeps a cut list ([`Cut`], timeline seconds and an
//! angle). [`program_segments`] turns it into which angle shows over a
//! range, and [`opacity_keys`] into each angle clip's opacity keys (stepped:
//! a key 6 ms before each change), so the editors need no new clip
//! type: every angle stays a clip on its own track and only the chosen one
//! is opaque. Both editors use this crate (the web through WASM); checked
//! against `fixtures/multicam/cases.json`.

/// Envelope values a second.
pub const ENVELOPE_RATE: f64 = 200.0;
/// Below this score the sound does not match well enough to sync.
pub const MIN_SYNC_SCORE: f64 = 0.2;
/// The step before a cut: just over the desktop's keyframe snap (5 ms),
/// so the two keys stay apart, and well under a frame, so no frame lands
/// inside it when cuts sit on frames.
pub const STEP: f64 = 0.006;

/// The onset envelope of mono `samples` at `rate`: the log RMS of each
/// hop-long frame (two hops wide), minus its mean, and only its rises.
pub fn onsets(samples: &[f32], rate: f64) -> Vec<f64> {
    let hop = ((rate / ENVELOPE_RATE).round() as usize).max(1);
    let width = 2 * hop;
    let frames = samples.len() / hop;
    let mut log_rms = Vec::with_capacity(frames);
    for frame in 0..frames {
        let start = frame * hop;
        let end = (start + width).min(samples.len());
        let energy: f64 = samples[start..end].iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / (end - start) as f64;
        log_rms.push((1e-4 + energy.sqrt()).ln());
    }
    let mut out = vec![0.0; frames];
    for index in 1..frames {
        out[index] = (log_rms[index] - log_rms[index - 1]).max(0.0);
    }
    if frames > 0 {
        let mean = out.iter().sum::<f64>() / frames as f64;
        for value in &mut out {
            *value -= mean;
        }
    }
    out
}

/// How well and where `other` lines up with `reference`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncResult {
    /// Seconds to add to the reference's start to place `other` (a sound
    /// `t` seconds into `other` is `t + lag` seconds into `reference`).
    pub lag: f64,
    /// Normalised correlation at the peak, about 0 (no match) to 1.
    pub score: f64,
}

/// The lag that best lines `other`'s onsets up with `reference`'s (both
/// from [`onsets`]), searched within `max_lag` seconds either way.
pub fn sync_lag(reference: &[f64], other: &[f64], max_lag: f64) -> Option<SyncResult> {
    if reference.is_empty() || other.is_empty() {
        return None;
    }
    let (n, m) = (reference.len(), other.len());
    let size = (n + m).next_power_of_two();
    let mut a: Vec<(f64, f64)> = (0..size).map(|i| (reference.get(i).copied().unwrap_or(0.0), 0.0)).collect();
    let mut b: Vec<(f64, f64)> = (0..size).map(|i| (other.get(i).copied().unwrap_or(0.0), 0.0)).collect();
    fft(&mut a, false);
    fft(&mut b, false);
    // c[L] = sum_j reference[j + L] * other[j]: A * conj(B).
    let mut c: Vec<(f64, f64)> = a.iter().zip(&b).map(|(&(ar, ai), &(br, bi))| (ar * br + ai * bi, ai * br - ar * bi)).collect();
    fft(&mut c, true);
    let at = |lag: i64| -> f64 {
        let index = if lag >= 0 { lag as usize } else { (size as i64 + lag) as usize };
        c[index].0
    };
    let reach = (max_lag * ENVELOPE_RATE).floor() as i64;
    let low = (-(m as i64) + 1).max(-reach);
    let high = (n as i64 - 1).min(reach);
    if low > high {
        return None;
    }
    let mut best = low;
    for lag in low..=high {
        if at(lag) > at(best) {
            best = lag;
        }
    }
    let norm = (reference.iter().map(|v| v * v).sum::<f64>() * other.iter().map(|v| v * v).sum::<f64>()).sqrt();
    if norm <= 0.0 {
        return None;
    }
    let peak = at(best);
    let shift = if best > low && best < high {
        let (left, right) = (at(best - 1), at(best + 1));
        let curve = left - 2.0 * peak + right;
        if curve < 0.0 { (0.5 * (left - right) / curve).clamp(-0.5, 0.5) } else { 0.0 }
    } else {
        0.0
    };
    Some(SyncResult { lag: (best as f64 + shift) / ENVELOPE_RATE, score: peak / norm })
}

/// In-place radix-2 FFT of `data` (length a power of two); `inverse`
/// scales by 1/n.
fn fft(data: &mut [(f64, f64)], inverse: bool) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (ur, ui) = data[start + k];
                let (xr, xi) = data[start + k + len / 2];
                let (vr, vi) = (xr * cr - xi * ci, xr * ci + xi * cr);
                data[start + k] = (ur + vr, ui + vi);
                data[start + k + len / 2] = (ur - vr, ui - vi);
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
    if inverse {
        for value in data.iter_mut() {
            value.0 /= n as f64;
            value.1 /= n as f64;
        }
    }
}

/// From `time` (timeline seconds) on, `angle` shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub time: f64,
    pub angle: usize,
}

/// `t:a,t:a` with times to the millisecond, sorted.
pub fn encode_cuts(cuts: &[Cut]) -> String {
    let mut sorted = cuts.to_vec();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    sorted.iter().map(|cut| format!("{:.3}:{}", cut.time, cut.angle)).collect::<Vec<_>>().join(",")
}

/// The stored cut list, sorted; bad entries are dropped.
pub fn parse_cuts(text: &str) -> Vec<Cut> {
    let mut cuts: Vec<Cut> = text
        .split(',')
        .filter_map(|part| {
            let (time, angle) = part.trim().split_once(':')?;
            let time: f64 = time.trim().parse().ok()?;
            let angle: usize = angle.trim().parse().ok()?;
            (time.is_finite() && time >= 0.0).then_some(Cut { time, angle })
        })
        .collect();
    cuts.sort_by(|a, b| a.time.total_cmp(&b.time));
    cuts
}

/// The cut list with `angle` from `time` on: a cut within half a
/// millisecond of `time` is replaced.
pub fn add_cut(cuts: &[Cut], time: f64, angle: usize) -> Vec<Cut> {
    let time = (time.max(0.0) * 1000.0).round() / 1000.0;
    let mut out: Vec<Cut> = cuts.iter().copied().filter(|cut| (cut.time - time).abs() >= 0.0005).collect();
    out.push(Cut { time, angle });
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

/// Which angle shows over `start..end`: runs of one angle, in order.
/// Before the first cut the first angle (0) shows.
pub fn program_segments(cuts: &[Cut], start: f64, end: f64) -> Vec<(f64, f64, usize)> {
    let mut sorted = cuts.to_vec();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    let mut angle = sorted.iter().filter(|cut| cut.time <= start).last().map_or(0, |cut| cut.angle);
    let mut segments: Vec<(f64, f64, usize)> = Vec::new();
    let mut from = start;
    for cut in sorted.iter().filter(|cut| cut.time > start && cut.time < end) {
        if cut.angle != angle {
            if cut.time > from {
                segments.push((from, cut.time, angle));
            }
            from = cut.time;
            angle = cut.angle;
        }
    }
    if end > from {
        segments.push((from, end, angle));
    }
    segments
}

/// The opacity keys (clip-local seconds, value) for the clip of `angle`
/// over `clip_start..clip_end` on the timeline: 1 where it shows, 0
/// elsewhere, stepping 6 ms before each change.
pub fn opacity_keys(cuts: &[Cut], angle: usize, clip_start: f64, clip_end: f64) -> Vec<(f64, f64)> {
    let segments = program_segments(cuts, clip_start, clip_end);
    let mut keys: Vec<(f64, f64)> = Vec::new();
    let mut current: Option<f64> = None;
    for (from, _, shown) in segments {
        let value = if shown == angle { 1.0 } else { 0.0 };
        match current {
            None => keys.push((from - clip_start, value)),
            Some(previous) if previous != value => {
                let at = from - clip_start;
                keys.push(((at - STEP).max(0.0), previous));
                keys.push((at, value));
            }
            Some(_) => {}
        }
        current = Some(value);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/multicam/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn floats(value: &Value) -> Vec<f64> {
        value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    }

    fn cuts(value: &Value) -> Vec<Cut> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|c| Cut { time: c[0].as_f64().unwrap(), angle: c[1].as_u64().unwrap() as usize })
            .collect()
    }

    #[test]
    fn onsets_and_sync_match_the_reference() {
        for case in cases()["sync"].as_array().unwrap() {
            let rate = case["rate"].as_f64().unwrap();
            let reference: Vec<f32> = floats(&case["reference"]).iter().map(|&v| v as f32).collect();
            let other: Vec<f32> = floats(&case["other"]).iter().map(|&v| v as f32).collect();
            let (a, b) = (onsets(&reference, rate), onsets(&other, rate));
            let expected = floats(&case["referenceOnsetsHead"]);
            for (got, want) in a.iter().zip(&expected) {
                assert!((got - want).abs() < 1e-5, "{} onsets: {got} vs {want}", case["name"]);
            }
            let result = sync_lag(&a, &b, case["maxLag"].as_f64().unwrap()).expect("a lag");
            assert!((result.lag - case["lag"].as_f64().unwrap()).abs() < 1e-6, "{}: lag {}", case["name"], result.lag);
            assert!((result.score - case["score"].as_f64().unwrap()).abs() < 1e-6, "{}: score {}", case["name"], result.score);
            assert!((result.lag - case["trueLag"].as_f64().unwrap()).abs() < 0.01, "{}: off the true lag", case["name"]);
        }
    }

    #[test]
    fn segments_and_keys_match_the_reference() {
        for case in cases()["program"].as_array().unwrap() {
            let list = cuts(&case["cuts"]);
            let (start, end) = (case["start"].as_f64().unwrap(), case["end"].as_f64().unwrap());
            let segments: Vec<(f64, f64, usize)> = case["segments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s[0].as_f64().unwrap(), s[1].as_f64().unwrap(), s[2].as_u64().unwrap() as usize))
                .collect();
            assert_eq!(program_segments(&list, start, end), segments, "{}", case["name"]);
            for clip in case["clips"].as_array().unwrap() {
                let keys = opacity_keys(&list, clip["angle"].as_u64().unwrap() as usize, clip["start"].as_f64().unwrap(), clip["end"].as_f64().unwrap());
                let expected: Vec<(f64, f64)> =
                    clip["keys"].as_array().unwrap().iter().map(|k| (k[0].as_f64().unwrap(), k[1].as_f64().unwrap())).collect();
                assert_eq!(keys.len(), expected.len(), "{} angle {}", case["name"], clip["angle"]);
                for (got, want) in keys.iter().zip(&expected) {
                    assert!((got.0 - want.0).abs() < 1e-9 && got.1 == want.1, "{}: {got:?} vs {want:?}", case["name"]);
                }
            }
        }
    }

    #[test]
    fn cut_lists_round_trip_and_replace() {
        for case in cases()["cutLists"].as_array().unwrap() {
            let parsed = parse_cuts(case["text"].as_str().unwrap());
            assert_eq!(parsed, cuts(&case["parsed"]), "{}", case["text"]);
            let added = add_cut(&parsed, case["addTime"].as_f64().unwrap(), case["addAngle"].as_u64().unwrap() as usize);
            assert_eq!(encode_cuts(&added), case["afterAdd"].as_str().unwrap());
        }
    }
}

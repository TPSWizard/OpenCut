//! Whole-mix loudness: brings a rendered mix to a target integrated
//! loudness (EBU R128 / BS.1770) and keeps its true peaks under a ceiling
//! with a look-ahead limiter, as both editors do at export.
//!
//! The limiter works on the true peak around each sample (the sample and
//! the 4× oversampled values next to it, as [`crate::true_peak`] measures
//! them): the gain each sample needs is held for 5 ms ahead so it is down
//! before a peak arrives, released over 50 ms, and smoothed by a 5 ms
//! average that never rises above what any sample needs. Both channels get
//! the same gain, so the stereo image holds. It aims 0.1 dB under the
//! ceiling, as the oversampled peak of the limited audio can sit a touch
//! above the per-sample estimate.
//!
//! Limiting takes some loudness off, so mastering runs up to three passes
//! from the original, raising the gain by what the last pass fell short,
//! until the result is within 0.2 LU of the target.

use std::collections::VecDeque;

use crate::{measure_loudness, true_peak_at, true_peak_phases};

/// How far ahead the limiter sees a peak coming, seconds.
pub const LIMITER_LOOKAHEAD_SECONDS: f64 = 0.005;
/// How quickly the limiter lets go after a peak, seconds (time constant).
pub const LIMITER_RELEASE_SECONDS: f64 = 0.05;
/// How far under the ceiling the limiter aims, dB.
pub const LIMITER_MARGIN_DB: f64 = 0.1;
const MAX_PASSES: usize = 3;
const TOLERANCE_LU: f64 = 0.2;

/// What mastering did.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterReport {
    pub input_lufs: f64,
    pub input_true_peak_dbtp: f64,
    /// The gain applied before the limiter, dB.
    pub gain_db: f64,
    pub output_lufs: f64,
    pub output_true_peak_dbtp: f64,
    /// Whether the limiter turned anything down.
    pub limited: bool,
    /// Passes run (0 for silence, which is left alone).
    pub passes: usize,
}

/// Limits the true peaks of `channels` (equal lengths) to `ceiling_dbtp`
/// (less [`LIMITER_MARGIN_DB`]), in place. Returns whether any gain was
/// taken off.
pub fn limit(channels: &mut [Vec<f32>], sample_rate: u32, ceiling_dbtp: f64) -> bool {
    let length = channels.iter().map(Vec::len).min().unwrap_or(0);
    if length == 0 {
        return false;
    }
    let phases = true_peak_phases(sample_rate);
    let ceiling = 10f64.powf((ceiling_dbtp - LIMITER_MARGIN_DB) / 20.0);
    let needed: Vec<f64> = (0..length)
        .map(|index| {
            let peak = channels.iter().map(|channel| true_peak_at(channel, index, &phases)).fold(0.0, f64::max);
            if peak > ceiling { ceiling / peak } else { 1.0 }
        })
        .collect();
    if needed.iter().all(|gain| *gain >= 1.0) {
        return false;
    }
    let ahead = ((LIMITER_LOOKAHEAD_SECONDS * sample_rate as f64).round() as usize).max(1);
    // The least gain needed from each sample to `ahead` samples later (a
    // sliding minimum, kept as a deque of rising values).
    let mut held = vec![1.0; length];
    let mut window: VecDeque<usize> = VecDeque::new();
    let mut next = 0;
    for (index, slot) in held.iter_mut().enumerate() {
        let last = (index + ahead).min(length - 1);
        while next <= last {
            while window.back().is_some_and(|&back| needed[back] >= needed[next]) {
                window.pop_back();
            }
            window.push_back(next);
            next += 1;
        }
        while window.front().is_some_and(|&front| front < index) {
            window.pop_front();
        }
        *slot = needed[*window.front().unwrap()];
    }
    // Release: rise back towards 1, never above what is held.
    let release = 1.0 - (-1.0 / (LIMITER_RELEASE_SECONDS * sample_rate as f64)).exp();
    let mut previous = 1.0;
    for value in held.iter_mut() {
        previous = value.min(previous + release * (1.0 - previous));
        *value = previous;
    }
    // Smooth over `ahead` samples; each value averaged covers this sample.
    let mut sum = 0.0;
    let mut smoothed = vec![1.0; length];
    for index in 0..length {
        sum += held[index];
        if index >= ahead {
            sum -= held[index - ahead];
        }
        smoothed[index] = sum / (index + 1).min(ahead) as f64;
    }
    for channel in channels.iter_mut() {
        for (sample, gain) in channel.iter_mut().zip(&smoothed) {
            *sample = (*sample as f64 * gain) as f32;
        }
    }
    true
}

fn scaled(channels: &[Vec<f32>], gain_db: f64) -> Vec<Vec<f32>> {
    let gain = 10f64.powf(gain_db / 20.0);
    channels.iter().map(|channel| channel.iter().map(|sample| (*sample as f64 * gain) as f32).collect()).collect()
}

/// Brings `channels` to `target_lufs` integrated with true peaks under
/// `ceiling_dbtp`, in place. Silence (nothing above the gate) is left as
/// it is.
pub fn master(channels: &mut [Vec<f32>], sample_rate: u32, target_lufs: f64, ceiling_dbtp: f64) -> MasterReport {
    let input = measure_loudness(channels, sample_rate);
    if !input.integrated_lufs.is_finite() {
        return MasterReport {
            input_lufs: input.integrated_lufs,
            input_true_peak_dbtp: input.true_peak_dbtp,
            gain_db: 0.0,
            output_lufs: input.integrated_lufs,
            output_true_peak_dbtp: input.true_peak_dbtp,
            limited: false,
            passes: 0,
        };
    }
    let mut gain_db = target_lufs - input.integrated_lufs;
    let mut passes = 0;
    loop {
        passes += 1;
        let mut work = scaled(channels, gain_db);
        let limited = limit(&mut work, sample_rate, ceiling_dbtp);
        let output = measure_loudness(&work, sample_rate);
        let short = target_lufs - output.integrated_lufs;
        if !limited || short.abs() <= TOLERANCE_LU || passes == MAX_PASSES || !short.is_finite() {
            for (channel, mastered) in channels.iter_mut().zip(work) {
                *channel = mastered;
            }
            return MasterReport {
                input_lufs: input.integrated_lufs,
                input_true_peak_dbtp: input.true_peak_dbtp,
                gain_db,
                output_lufs: output.integrated_lufs,
                output_true_peak_dbtp: output.true_peak_dbtp,
                limited,
                passes,
            };
        }
        gain_db += short;
    }
}

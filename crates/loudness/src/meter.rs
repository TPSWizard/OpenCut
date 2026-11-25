//! A live loudness meter for playback: samples go in as they are played,
//! in chunks of any size, and the meter reads the momentary (last 400 ms)
//! and short-term (last 3 s) loudness after BS.1770, and the sample peak of
//! the last 300 ms. The audio is K-weighted as it arrives and summed into
//! 100 ms blocks, so the readings do not depend on how it was chunked.
//! Until 400 ms (or 3 s) has played, the blocks so far are used.

use std::collections::VecDeque;

use crate::measure::{Biquad, k_weighting, lufs};

const BLOCK_SECONDS: f64 = 0.1;
const MOMENTARY_BLOCKS: usize = 4;
const SHORT_TERM_BLOCKS: usize = 30;
const PEAK_BLOCKS: usize = 3;

/// What the meter reads now; `-inf` before anything was heard.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterReadings {
    pub momentary_lufs: f64,
    pub short_term_lufs: f64,
    /// Sample peak of the last 300 ms, dBFS.
    pub peak_dbfs: f64,
}

#[derive(Clone, Debug)]
pub struct Meter {
    channels: usize,
    stages: [Biquad; 2],
    /// Per channel and stage: x1, x2, y1, y2.
    state: Vec<[[f64; 4]; 2]>,
    block_length: usize,
    block_sum: f64,
    block_count: usize,
    block_peak: f64,
    /// Finished blocks, newest last: (summed mean square, sample peak).
    blocks: VecDeque<(f64, f64)>,
}

impl Meter {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        Self {
            channels,
            stages: k_weighting(sample_rate as f64),
            state: vec![[[0.0; 4]; 2]; channels],
            block_length: ((BLOCK_SECONDS * sample_rate as f64).round() as usize).max(1),
            block_sum: 0.0,
            block_count: 0,
            block_peak: 0.0,
            blocks: VecDeque::with_capacity(SHORT_TERM_BLOCKS + 1),
        }
    }

    /// Forgets what was heard (after a seek or a stop).
    pub fn reset(&mut self) {
        for channel in &mut self.state {
            *channel = [[0.0; 4]; 2];
        }
        self.block_sum = 0.0;
        self.block_count = 0;
        self.block_peak = 0.0;
        self.blocks.clear();
    }

    fn weigh(&mut self, channel: usize, sample: f64) -> f64 {
        let mut value = sample;
        for (stage, memory) in self.stages.iter().zip(self.state[channel].iter_mut()) {
            let [x1, x2, y1, y2] = *memory;
            let y0 = stage.b0 * value + stage.b1 * x1 + stage.b2 * x2 - stage.a1 * y1 - stage.a2 * y2;
            *memory = [value, x1, y0, y1];
            value = y0;
        }
        value
    }

    fn frame(&mut self, samples: impl Iterator<Item = f32>) {
        for (channel, sample) in samples.enumerate().take(self.channels) {
            let sample = sample as f64;
            self.block_peak = self.block_peak.max(sample.abs());
            let weighted = self.weigh(channel, sample);
            self.block_sum += weighted * weighted;
        }
        self.block_count += 1;
        if self.block_count == self.block_length {
            self.blocks.push_back((self.block_sum / self.block_length as f64, self.block_peak));
            if self.blocks.len() > SHORT_TERM_BLOCKS {
                self.blocks.pop_front();
            }
            self.block_sum = 0.0;
            self.block_count = 0;
            self.block_peak = 0.0;
        }
    }

    /// Interleaved frames (`channels` samples each).
    pub fn push_interleaved(&mut self, samples: &[f32]) {
        for frame in samples.chunks_exact(self.channels) {
            self.frame(frame.iter().copied());
        }
    }

    /// One slice per channel, equal lengths.
    pub fn push_planar(&mut self, planes: &[&[f32]]) {
        let length = planes.iter().map(|plane| plane.len()).min().unwrap_or(0);
        for index in 0..length {
            self.frame(planes.iter().map(|plane| plane[index]));
        }
    }

    pub fn readings(&self) -> MeterReadings {
        let loudness = |count: usize| {
            let recent: Vec<f64> = self.blocks.iter().rev().take(count).map(|block| block.0).collect();
            if recent.is_empty() { f64::NEG_INFINITY } else { lufs(recent.iter().sum::<f64>() / recent.len() as f64) }
        };
        let peak = self.blocks.iter().rev().take(PEAK_BLOCKS).map(|block| block.1).fold(self.block_peak, f64::max);
        MeterReadings {
            momentary_lufs: loudness(MOMENTARY_BLOCKS),
            short_term_lufs: loudness(SHORT_TERM_BLOCKS),
            peak_dbfs: if peak > 0.0 { 20.0 * peak.log10() } else { f64::NEG_INFINITY },
        }
    }
}

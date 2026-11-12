//! Tempo and beat tracking from onset energy, the same arithmetic as the
//! web app's `src/beats/track.ts`: log-energy flux per 5 ms hop with its
//! local mean removed, the tempo from the autocorrelation peak between 60
//! and 200 BPM (weighted towards 120), the phase from the offset that
//! lines the beat grid up with the strongest onsets.

#[derive(Clone, Debug, PartialEq)]
pub struct BeatAnalysis {
    /// Estimated tempo, or none when nothing periodic was found.
    pub bpm: Option<f64>,
    /// Autocorrelation peak over the zero lag, 0 to 1.
    pub confidence: f64,
    /// Beat times in seconds from the start of the samples.
    pub beats: Vec<f64>,
    pub seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatOptions {
    pub min_bpm: f64,
    pub max_bpm: f64,
    /// The tempo the weighting prefers when the autocorrelation is ambiguous.
    pub preferred_bpm: f64,
}

impl Default for BeatOptions {
    fn default() -> Self {
        Self { min_bpm: 60.0, max_bpm: 200.0, preferred_bpm: 120.0 }
    }
}

const HOP_SECONDS: f64 = 0.005;
const WINDOW_HOPS: usize = 4;
const LOCAL_MEAN_SECONDS: f64 = 0.5;
const MIN_CONFIDENCE: f64 = 0.05;
/// Onsets weaker than this (about half a decibel of flux) are noise, not beats.
const MIN_ONSET: f64 = 0.05;

/// Log-energy flux per hop with the local mean removed, clipped at zero.
pub fn onset_strength(samples: &[f32], sample_rate: u32) -> Vec<f64> {
    let hop = ((sample_rate as f64 * HOP_SECONDS).round() as usize).max(1);
    let window = hop * WINDOW_HOPS;
    let frames = if samples.len() >= window { (samples.len() - window) / hop + 1 } else { 0 };
    let energy: Vec<f64> = (0..frames)
        .map(|frame| {
            let start = frame * hop;
            samples[start..start + window].iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / window as f64
        })
        .collect();
    let mut flux = vec![0.0; frames];
    for frame in 1..frames {
        flux[frame] = ((energy[frame] + 1e-10).ln() - (energy[frame - 1] + 1e-10).ln()).max(0.0);
    }
    let radius = (LOCAL_MEAN_SECONDS / HOP_SECONDS / 2.0).round() as usize;
    let mut onset = vec![0.0; frames];
    let mut window_sum = 0.0;
    let mut left = 0usize;
    let mut right: isize = -1;
    for frame in 0..frames {
        while right < (frames as isize - 1).min(frame as isize + radius as isize) {
            right += 1;
            window_sum += flux[right as usize];
        }
        while (left as isize) < frame as isize - radius as isize {
            window_sum -= flux[left];
            left += 1;
        }
        let mean = window_sum / (right - left as isize + 1) as f64;
        onset[frame] = (flux[frame] - mean).max(0.0);
    }
    onset
}

fn autocorrelation(onset: &[f64], lag: usize) -> f64 {
    if onset.len() <= lag {
        return 0.0;
    }
    let sum: f64 = (0..onset.len() - lag).map(|index| onset[index] * onset[index + lag]).sum();
    sum / (onset.len() - lag).max(1) as f64
}

/// Finds the tempo and beats of mono samples.
pub fn track_beats(samples: &[f32], sample_rate: u32, options: &BeatOptions) -> BeatAnalysis {
    let seconds = samples.len() as f64 / sample_rate as f64;
    let onset = onset_strength(samples, sample_rate);
    let empty = BeatAnalysis { bpm: None, confidence: 0.0, beats: Vec::new(), seconds };
    let zero = autocorrelation(&onset, 0);
    let strongest = onset.iter().copied().fold(0.0, f64::max);
    if (onset.len() as f64) < 2.0 / HOP_SECONDS || zero <= 0.0 || strongest < MIN_ONSET {
        return empty;
    }
    let min_lag = ((60.0 / options.max_bpm / HOP_SECONDS).round() as usize).max(1);
    let max_lag = ((60.0 / options.min_bpm / HOP_SECONDS).round() as usize).min(onset.len() - 1);
    let preferred_lag = 60.0 / options.preferred_bpm / HOP_SECONDS;
    let mut best_lag = 0usize;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_raw = 0.0;
    let mut correlations = std::collections::HashMap::new();
    for lag in min_lag..=max_lag {
        let raw = autocorrelation(&onset, lag);
        correlations.insert(lag, raw);
        let weight = (-0.5 * ((lag as f64 / preferred_lag).log2()).powi(2)).exp();
        let score = raw * weight;
        if score > best_score {
            best_score = score;
            best_lag = lag;
            best_raw = raw;
        }
    }
    let confidence = best_raw / zero;
    if best_lag == 0 || confidence < MIN_CONFIDENCE {
        return empty;
    }
    let before = correlations.get(&(best_lag - 1)).copied().unwrap_or(best_raw);
    let after = correlations.get(&(best_lag + 1)).copied().unwrap_or(best_raw);
    let denominator = before - 2.0 * best_raw + after;
    let shift = if denominator != 0.0 { (0.5 * (before - after) / denominator).clamp(-0.5, 0.5) } else { 0.0 };
    let period = best_lag as f64 + shift;
    let bpm = 60.0 / (period * HOP_SECONDS);

    let mut best_offset = 0usize;
    let mut best_phase = f64::NEG_INFINITY;
    for offset in 0..best_lag {
        let sum: f64 = (offset..onset.len()).step_by(best_lag).map(|frame| onset[frame]).sum();
        if sum > best_phase {
            best_phase = sum;
            best_offset = offset;
        }
    }
    let mut beats = Vec::new();
    let mut beat = 0.0;
    loop {
        let time = (best_offset as f64 + beat * period) * HOP_SECONDS;
        if time >= seconds {
            break;
        }
        beats.push((time * 10_000.0).round() / 10_000.0);
        beat += 1.0;
    }
    BeatAnalysis { bpm: Some(bpm), confidence, beats, seconds }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    fn clicks(bpm: f64, seconds: f64, phase: f64) -> Vec<f32> {
        let period = 60.0 / bpm;
        (0..(seconds * RATE as f64).round() as usize)
            .map(|index| {
                let t = index as f64 / RATE as f64;
                let since = ((t - phase) % period + period) % period;
                let mut value = 0.01 * (2.0 * std::f64::consts::PI * 220.0 * t).sin();
                if t >= phase && since < 0.02 {
                    value += 0.8 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
                }
                value as f32
            })
            .collect()
    }

    #[test]
    fn finds_the_tempo_and_phase_of_a_click_track() {
        let result = track_beats(&clicks(100.0, 12.0, 0.25), RATE, &BeatOptions::default());
        let bpm = result.bpm.expect("a tempo");
        assert!((bpm - 100.0).abs() < 1.5, "{bpm}");
        assert!(result.confidence > 0.3, "{}", result.confidence);
        assert!(result.beats.len() > 15);
        for beat in &result.beats {
            let offset = ((beat - 0.25) % 0.6 + 0.6) % 0.6;
            assert!(offset.min(0.6 - offset) < 0.03, "{beat}");
        }
        assert_eq!(result.seconds, 12.0);
    }

    #[test]
    fn silence_and_a_steady_tone_give_nothing() {
        assert_eq!(track_beats(&vec![0.0; RATE as usize * 5], RATE, &BeatOptions::default()).bpm, None);
        let tone: Vec<f32> =
            (0..RATE as usize * 5).map(|i| (0.5 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / RATE as f64).sin()) as f32).collect();
        let result = track_beats(&tone, RATE, &BeatOptions::default());
        assert!(result.beats.is_empty() || result.confidence < 0.2, "{}", result.confidence);
        let onset = onset_strength(&clicks(120.0, 2.0, 0.0), RATE);
        assert!(onset.len() > 380 && onset.len() < 400, "{}", onset.len());
    }
}

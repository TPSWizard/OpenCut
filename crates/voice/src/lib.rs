//! Voice effects on one channel of raw samples, the same in both editors
//! (the desktop calls this directly; the web through `opencut-wasm`'s
//! `applyVoiceEffect`). Every effect keeps the length of its input, so a
//! processed file lines up with the clip it replaces.
//!
//! - Pitch (deeper, higher, chipmunk, monster): a phase vocoder stretches
//!   time by the pitch ratio, then the result is resampled back to the
//!   original length, which moves every frequency by that ratio.
//! - Robot: the same short-time spectra with every phase set to zero, so
//!   each frame is a symmetric pulse; frames a fixed hop apart give a
//!   monotone buzz at `sample_rate / hop` (about 110 Hz) with the voice's
//!   spectral shape, the classic vocoder robot.
//! - Echo: a feedback delay (300 ms, three audible repeats).
//! - Radio: telephone band (two high-passes at 300 Hz, two low-passes at
//!   3.4 kHz) with a little soft-clipped drive.
//! - Hall: a Schroeder reverb (four combs in parallel, two all-passes).
//!
//! [`normalize_peak`] then keeps the channels of a clip together under
//! full scale. [`enhance_voice`] (in `enhance.rs`) is the one-click
//! clean-up chain for speech.

use std::f32::consts::PI;

mod enhance;
pub use enhance::{ENHANCE_TARGET_LUFS, EnhanceReport, enhance_voice, integrated_loudness};

/// `(id, label)` of every effect, in menu order.
pub const VOICE_EFFECTS: &[(&str, &str)] = &[
    ("deeper", "Deeper"),
    ("higher", "Higher"),
    ("chipmunk", "Chipmunk"),
    ("monster", "Monster"),
    ("robot", "Robot"),
    ("echo", "Echo"),
    ("radio", "Radio"),
    ("hall", "Hall"),
];

/// The semitones a pitch effect moves the voice by, or `None` for the others.
pub fn pitch_semitones(effect: &str) -> Option<f32> {
    match effect {
        "deeper" => Some(-4.0),
        "higher" => Some(4.0),
        "chipmunk" => Some(9.0),
        "monster" => Some(-9.0),
        _ => None,
    }
}

pub fn is_voice_effect(effect: &str) -> bool {
    VOICE_EFFECTS.iter().any(|(id, _)| *id == effect)
}

/// One channel through `effect`; `None` for an unknown effect. The output
/// has the input's length and is not normalised (see [`normalize_peak`]).
pub fn apply_voice_effect(effect: &str, samples: &[f32], sample_rate: f32) -> Option<Vec<f32>> {
    if let Some(semitones) = pitch_semitones(effect) {
        return Some(pitch_shift(samples, 2f32.powf(semitones / 12.0)));
    }
    Some(match effect {
        "robot" => robot(samples, sample_rate),
        "echo" => echo(samples, sample_rate),
        "radio" => radio(samples, sample_rate),
        "hall" => hall(samples, sample_rate),
        _ => return None,
    })
}

/// Scales all channels together so the loudest sample is at most `ceiling`
/// (never louder than they were); returns the gain used.
pub fn normalize_peak(channels: &mut [Vec<f32>], ceiling: f32) -> f32 {
    let peak = channels.iter().flatten().fold(0f32, |peak, sample| peak.max(sample.abs()));
    if peak <= ceiling || peak == 0.0 {
        return 1.0;
    }
    let gain = ceiling / peak;
    for sample in channels.iter_mut().flatten() {
        *sample *= gain;
    }
    gain
}

// --- FFT -------------------------------------------------------------------

/// In-place radix-2 FFT of `re`/`im` (length a power of two); `inverse`
/// runs the inverse transform without the 1/N scale.
fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut size = 2;
    while size <= n {
        let angle = sign * 2.0 * std::f64::consts::PI / size as f64;
        let (step_re, step_im) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(size) {
            let (mut w_re, mut w_im) = (1f64, 0f64);
            for k in 0..size / 2 {
                let (a, b) = (start + k, start + k + size / 2);
                let (b_re, b_im) = (re[b] as f64 * w_re - im[b] as f64 * w_im, re[b] as f64 * w_im + im[b] as f64 * w_re);
                re[b] = (re[a] as f64 - b_re) as f32;
                im[b] = (im[a] as f64 - b_im) as f32;
                re[a] = (re[a] as f64 + b_re) as f32;
                im[a] = (im[a] as f64 + b_im) as f32;
                (w_re, w_im) = (w_re * step_re - w_im * step_im, w_re * step_im + w_im * step_re);
            }
        }
        size <<= 1;
    }
}

fn hann(size: usize) -> Vec<f32> {
    (0..size).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos()).collect()
}

fn wrap_phase(phase: f32) -> f32 {
    let two_pi = 2.0 * PI;
    phase - two_pi * ((phase + PI) / two_pi).floor()
}

// --- Pitch -----------------------------------------------------------------

const FRAME: usize = 2048;
const HOP: usize = FRAME / 4;

/// Phase-vocoder time stretch: analysis frames `analysis_hop` apart are
/// laid `synthesis_hop` apart with their phases advanced to match.
fn stretch(samples: &[f32], analysis_hop: usize, synthesis_hop: usize) -> Vec<f32> {
    let window = hann(FRAME);
    let frames = samples.len().div_ceil(analysis_hop) + 1;
    let out_len = frames * synthesis_hop + FRAME;
    let mut out = vec![0f32; out_len];
    let mut weight = vec![0f32; out_len];
    let bins = FRAME / 2 + 1;
    let mut last_phase = vec![0f32; bins];
    let mut sum_phase = vec![0f32; bins];
    let (mut re, mut im) = (vec![0f32; FRAME], vec![0f32; FRAME]);
    // Frames start half a window before the signal so its start is covered.
    let offset = FRAME / 2;
    for frame in 0..frames {
        let start = (frame * analysis_hop) as isize - offset as isize;
        for i in 0..FRAME {
            let index = start + i as isize;
            re[i] = if index >= 0 && (index as usize) < samples.len() { samples[index as usize] * window[i] } else { 0.0 };
            im[i] = 0.0;
        }
        fft(&mut re, &mut im, false);
        for k in 0..bins {
            let magnitude = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let phase = im[k].atan2(re[k]);
            let expected = 2.0 * PI * k as f32 * analysis_hop as f32 / FRAME as f32;
            let deviation = wrap_phase(phase - last_phase[k] - expected);
            last_phase[k] = phase;
            let advance = (expected + deviation) * synthesis_hop as f32 / analysis_hop as f32;
            sum_phase[k] = if frame == 0 { phase } else { wrap_phase(sum_phase[k] + advance) };
            re[k] = magnitude * sum_phase[k].cos();
            im[k] = magnitude * sum_phase[k].sin();
        }
        for k in 1..FRAME / 2 {
            re[FRAME - k] = re[k];
            im[FRAME - k] = -im[k];
        }
        fft(&mut re, &mut im, true);
        let at = frame * synthesis_hop;
        for i in 0..FRAME {
            out[at + i] += re[i] / FRAME as f32 * window[i];
            weight[at + i] += window[i] * window[i];
        }
    }
    for (sample, weight) in out.iter_mut().zip(&weight) {
        if *weight > 1e-3 {
            *sample /= weight;
        }
    }
    // Undo the half-window lead, scaled to the synthesis spacing.
    let lead = offset * synthesis_hop / analysis_hop;
    out.drain(..lead.min(out.len()));
    out
}

/// Moves every frequency by `ratio` (2.0 = an octave up), keeping the length.
pub fn pitch_shift(samples: &[f32], ratio: f32) -> Vec<f32> {
    if samples.is_empty() || (ratio - 1.0).abs() < 1e-4 {
        return samples.to_vec();
    }
    let synthesis_hop = ((HOP as f32 * ratio).round() as usize).max(1);
    let ratio = synthesis_hop as f32 / HOP as f32;
    let stretched = stretch(samples, HOP, synthesis_hop);
    (0..samples.len())
        .map(|i| {
            let position = i as f32 * ratio;
            let index = position.floor() as usize;
            let fraction = position - index as f32;
            let a = stretched.get(index).copied().unwrap_or(0.0);
            let b = stretched.get(index + 1).copied().unwrap_or(0.0);
            a + (b - a) * fraction
        })
        .collect()
}

// --- Robot -----------------------------------------------------------------

/// Zero-phase resynthesis at a fixed hop: a monotone buzz near 110 Hz.
pub fn robot(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    const SIZE: usize = 1024;
    let hop = ((sample_rate / 110.0).round() as usize).clamp(64, SIZE / 2);
    let window = hann(SIZE);
    let mut out = vec![0f32; samples.len() + SIZE];
    let mut weight = vec![0f32; samples.len() + SIZE];
    let (mut re, mut im) = (vec![0f32; SIZE], vec![0f32; SIZE]);
    let mut start = 0;
    while start < samples.len() + SIZE / 2 {
        for i in 0..SIZE {
            let index = (start + i).checked_sub(SIZE / 2);
            re[i] = index.and_then(|index| samples.get(index)).copied().unwrap_or(0.0) * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im, false);
        for k in 0..SIZE {
            re[k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
            im[k] = 0.0;
        }
        fft(&mut re, &mut im, true);
        // A zero-phase frame is a pulse at index 0; centre it in the window.
        for i in 0..SIZE {
            let value = re[(i + SIZE / 2) % SIZE] / SIZE as f32;
            if let Some(target) = (start + i).checked_sub(SIZE / 2) {
                if target < out.len() {
                    out[target] += value * window[i];
                    weight[target] += window[i] * window[i];
                }
            }
        }
        start += hop;
    }
    out.truncate(samples.len());
    weight.truncate(samples.len());
    for (sample, weight) in out.iter_mut().zip(&weight) {
        if *weight > 1e-3 {
            *sample /= weight;
        }
    }
    out
}

// --- Echo, radio, hall -----------------------------------------------------

pub fn echo(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    let delay = (0.3 * sample_rate).round().max(1.0) as usize;
    let feedback = 0.4;
    let mut out = samples.to_vec();
    for i in delay..out.len() {
        out[i] += out[i - delay] * feedback;
    }
    out
}

/// A biquad (RBJ cookbook) run over the samples.
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn new(high_pass: bool, frequency: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * (frequency / sample_rate).min(0.49);
        let (sin, cos) = omega.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self { b: b.map(|value| value / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], x: [0.0; 2], y: [0.0; 2] }
    }

    fn run(&mut self, input: f32) -> f32 {
        let output = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [output, self.y[0]];
        output
    }
}

pub fn radio(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    let mut filters = [
        Biquad::new(true, 300.0, sample_rate),
        Biquad::new(true, 300.0, sample_rate),
        Biquad::new(false, 3400.0, sample_rate),
        Biquad::new(false, 3400.0, sample_rate),
    ];
    let drive = 2.5;
    samples
        .iter()
        .map(|sample| {
            let filtered = filters.iter_mut().fold(*sample, |value, filter| filter.run(value));
            (filtered * drive).tanh() / drive.tanh()
        })
        .collect()
}

pub fn hall(samples: &[f32], sample_rate: f32) -> Vec<f32> {
    let scale = sample_rate / 44_100.0;
    let combs: Vec<(usize, f32)> = [(1557, 0.84), (1617, 0.83), (1491, 0.85), (1422, 0.86)]
        .iter()
        .map(|(length, feedback)| (((*length as f32) * scale).round().max(1.0) as usize, *feedback))
        .collect();
    let mut wet = vec![0f32; samples.len()];
    for (length, feedback) in &combs {
        let mut buffer = vec![0f32; *length];
        let mut damped = 0f32;
        for (i, sample) in samples.iter().enumerate() {
            let index = i % length;
            let delayed = buffer[index];
            // A one-pole low-pass in the loop: the tail darkens like a room.
            damped = delayed * 0.7 + damped * 0.3;
            buffer[index] = sample + damped * feedback;
            wet[i] += delayed / combs.len() as f32;
        }
    }
    for length in [225usize, 556] {
        let length = ((length as f32) * scale).round().max(1.0) as usize;
        let mut buffer = vec![0f32; length];
        for (i, sample) in wet.iter_mut().enumerate() {
            let index = i % length;
            let delayed = buffer[index];
            let output = -*sample + delayed;
            buffer[index] = *sample + delayed * 0.5;
            *sample = output;
        }
    }
    samples.iter().zip(&wet).map(|(dry, wet)| dry * 0.7 + wet * 0.6).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn sine(frequency: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE * seconds) as usize).map(|i| (2.0 * PI * frequency * i as f32 / RATE).sin() * 0.5).collect()
    }

    /// Power at `frequency` (a single-bin DFT over the middle of the signal).
    fn power(samples: &[f32], frequency: f32) -> f32 {
        let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
        let (mut re, mut im) = (0f32, 0f32);
        for (i, sample) in middle.iter().enumerate() {
            let angle = 2.0 * PI * frequency * i as f32 / RATE;
            re += sample * angle.cos();
            im += sample * angle.sin();
        }
        (re * re + im * im).sqrt() / middle.len() as f32
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn fft_round_trips() {
        let mut re: Vec<f32> = (0..16).map(|i| (i as f32 * 0.7).sin()).collect();
        let original = re.clone();
        let mut im = vec![0f32; 16];
        fft(&mut re, &mut im, false);
        fft(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&original) {
            assert!((a / 16.0 - b).abs() < 1e-5);
        }
    }

    #[test]
    fn pitch_moves_the_frequency_and_keeps_the_length() {
        let input = sine(440.0, 1.0);
        let up = pitch_shift(&input, 2f32.powf(4.0 / 12.0));
        assert_eq!(up.len(), input.len());
        let target = 440.0 * 2f32.powf(4.0 / 12.0); // 554 Hz
        assert!(power(&up, target) > 5.0 * power(&up, 440.0), "{} vs {}", power(&up, target), power(&up, 440.0));
        assert!(power(&up, target) > 0.1, "kept most of the level: {}", power(&up, target));
        let down = apply_voice_effect("deeper", &input, RATE).unwrap();
        let target = 440.0 * 2f32.powf(-4.0 / 12.0); // 349 Hz
        assert!(power(&down, target) > 5.0 * power(&down, 440.0));
        assert!((rms(&down) / rms(&input) - 1.0).abs() < 0.25, "level {}", rms(&down) / rms(&input));
    }

    #[test]
    fn pitch_keeps_sounds_where_they_were() {
        // Silence, then a tone from 0.5 s: after shifting it still starts at 0.5 s.
        let mut input = vec![0f32; (RATE * 0.5) as usize];
        input.extend(sine(440.0, 0.5));
        for semitones in [-9.0f32, 9.0] {
            let out = pitch_shift(&input, 2f32.powf(semitones / 12.0));
            let at = |from: f32, to: f32| rms(&out[(from * RATE) as usize..(to * RATE) as usize]);
            assert!(at(0.40, 0.47) < 0.02, "{semitones}: early {}", at(0.40, 0.47));
            assert!(at(0.53, 0.60) > 0.2, "{semitones}: late {}", at(0.53, 0.60));
        }
    }

    #[test]
    fn robot_buzzes_at_a_fixed_pitch() {
        let input = sine(300.0, 1.0);
        let out = robot(&input, RATE);
        assert_eq!(out.len(), input.len());
        let hop = (RATE / 110.0).round();
        let buzz = RATE / hop;
        // The energy lands on harmonics of the buzz nearest the voice, not on 300 Hz itself.
        let harmonic = (300.0 / buzz).round() * buzz;
        assert!(power(&out, harmonic) > 2.0 * power(&out, 300.0), "{} vs {}", power(&out, harmonic), power(&out, 300.0));
    }

    #[test]
    fn echo_repeats_after_300_ms() {
        let mut click = vec![0f32; RATE as usize];
        click[0] = 1.0;
        let out = echo(&click, RATE);
        let delay = (0.3 * RATE) as usize;
        assert!((out[delay] - 0.4).abs() < 1e-6 && (out[2 * delay] - 0.16).abs() < 1e-6);
        assert_eq!(out.len(), click.len());
    }

    #[test]
    fn radio_keeps_the_telephone_band() {
        let low = rms(&radio(&sine(100.0, 0.5), RATE));
        let mid = rms(&radio(&sine(1000.0, 0.5), RATE));
        let high = rms(&radio(&sine(8000.0, 0.5), RATE));
        assert!(mid > 4.0 * low && mid > 4.0 * high, "{low} {mid} {high}");
    }

    #[test]
    fn hall_leaves_a_tail() {
        let mut burst = sine(500.0, 0.1);
        burst.resize(RATE as usize, 0.0);
        let out = hall(&burst, RATE);
        let tail = rms(&out[(0.3 * RATE) as usize..(0.6 * RATE) as usize]);
        assert!(tail > 0.005, "tail {tail}");
        assert_eq!(rms(&burst[(0.3 * RATE) as usize..(0.6 * RATE) as usize]), 0.0);
    }

    #[test]
    fn every_effect_keeps_length_and_silence() {
        let silence = vec![0f32; 10_000];
        for (id, _) in VOICE_EFFECTS {
            let out = apply_voice_effect(id, &silence, RATE).unwrap();
            assert_eq!(out.len(), silence.len(), "{id}");
            assert!(out.iter().all(|s| s.abs() < 1e-6), "{id}");
        }
        assert!(apply_voice_effect("nope", &silence, RATE).is_none());
        let mut channels = vec![vec![2.0, -1.0], vec![0.5, 4.0]];
        assert!((normalize_peak(&mut channels, 0.98) - 0.245).abs() < 1e-6);
        assert!((channels[1][1] - 0.98).abs() < 1e-6);
    }
}

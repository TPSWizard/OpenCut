//! Enhance voice: one pass that makes speech clearer and even, the same in
//! both editors (the desktop calls [`enhance_voice`] directly; the web
//! through `opencut-wasm`'s `enhanceVoice`). In order:
//!
//! 1. High-pass at 80 Hz (fourth order): rumble, wind and handling noise.
//! 2. Noise reduction: the spectral subtraction of the web editor's noise
//!    reduction tool (`apps/web/src/denoise/spectral.ts`), with a gentler
//!    floor. It runs only when the clip has pauses that sit well below the
//!    speech, since a clip without them has no noise to learn from.
//! 3. EQ: a 2 dB cut at 250 Hz (boxiness) and a 3 dB presence lift at
//!    3.5 kHz (intelligibility).
//! 4. De-esser: the 5 kHz-and-up band is turned down while it carries
//!    most of the energy (an "s" or "sh"), by at most 9 dB.
//! 5. Compressor: 3:1 above −26 dBFS with a 6 dB knee, linked across
//!    channels, after the clip is first brought to −20 LUFS so the threshold
//!    means the same for quiet and loud recordings; below −48 dBFS RMS a gentle
//!    expander keeps the pauses down.
//! 6. Loudness: gain to −16 LUFS integrated (BS.1770, the arithmetic of
//!    `editor::loudness`), then a peak limiter at −1 dBFS.
//!
//! Every stage keeps the length of its input.

use std::f64::consts::PI;

/// The integrated loudness the clip ends at.
pub const ENHANCE_TARGET_LUFS: f64 = -16.0;
/// The level the compressor sees the clip at.
const COMPRESSOR_INPUT_LUFS: f64 = -20.0;
/// The sample peak the limiter holds (−1 dBFS).
const PEAK_CEILING: f64 = 0.891_250_938_133_745_5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnhanceReport {
    /// Integrated loudness before and after, in LUFS (`-inf` for silence).
    pub input_lufs: f64,
    pub output_lufs: f64,
    /// Whether the noise reduction stage ran (the clip had quiet pauses
    /// with steady background in them).
    pub noise_reduced: bool,
}

/// Runs the whole chain over the channels of one clip (equal lengths), in place.
pub fn enhance_voice(channels: &mut [Vec<f32>], sample_rate: f32) -> EnhanceReport {
    let rate = sample_rate as f64;
    let input_lufs = integrated_loudness(channels, rate);
    if channels.is_empty() || !input_lufs.is_finite() {
        return EnhanceReport { input_lufs, output_lufs: input_lufs, noise_reduced: false };
    }
    let mut planes: Vec<Vec<f64>> = channels.iter().map(|channel| channel.iter().map(|s| *s as f64).collect()).collect();
    for plane in planes.iter_mut() {
        let mut stages = [Biquad::high_pass(80.0, 0.541_196, rate), Biquad::high_pass(80.0, 1.306_563, rate)];
        run(plane, &mut stages);
    }
    let noise_reduced = has_steady_pauses(&planes, rate);
    if noise_reduced {
        for plane in planes.iter_mut() {
            *plane = denoise(plane, &DenoiseSettings::ENHANCE);
        }
    }
    for plane in planes.iter_mut() {
        let mut stages = [Biquad::peaking(250.0, 1.0, -2.0, rate), Biquad::peaking(3500.0, 0.9, 3.0, rate)];
        run(plane, &mut stages);
    }
    apply_gain_to(&mut planes, rate, COMPRESSOR_INPUT_LUFS);
    de_ess(&mut planes, rate);
    compress(&mut planes, rate);
    apply_gain_to(&mut planes, rate, ENHANCE_TARGET_LUFS);
    limit(&mut planes, rate);
    for (channel, plane) in channels.iter_mut().zip(&planes) {
        for (sample, value) in channel.iter_mut().zip(plane) {
            *sample = *value as f32;
        }
    }
    let output_lufs = integrated_loudness(channels, rate);
    EnhanceReport { input_lufs, output_lufs, noise_reduced }
}

// --- Filters ---------------------------------------------------------------

/// An RBJ-cookbook biquad in f64.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    state: [f64; 4],
}

impl Biquad {
    fn from(b: [f64; 3], a0: f64, a: [f64; 2]) -> Self {
        Self { b: b.map(|value| value / a0), a: a.map(|value| value / a0), state: [0.0; 4] }
    }

    fn omega(frequency: f64, rate: f64) -> f64 {
        2.0 * PI * (frequency / rate).min(0.45)
    }

    fn high_pass(frequency: f64, q: f64, rate: f64) -> Self {
        let (sin, cos) = Self::omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], 1.0 + alpha, [-2.0 * cos, 1.0 - alpha])
    }

    fn peaking(frequency: f64, q: f64, gain_db: f64, rate: f64) -> Self {
        let (sin, cos) = Self::omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);
        Self::from([1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a], 1.0 + alpha / a, [-2.0 * cos, 1.0 - alpha / a])
    }

    fn tick(&mut self, x: f64) -> f64 {
        let [x1, x2, y1, y2] = self.state;
        let y = self.b[0] * x + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
        self.state = [x, x1, y, y1];
        y
    }
}

fn run(samples: &mut [f64], stages: &mut [Biquad]) {
    for sample in samples.iter_mut() {
        *sample = stages.iter_mut().fold(*sample, |value, stage| stage.tick(value));
    }
}

/// A one-pole envelope coefficient for a time constant in seconds.
fn coefficient(seconds: f64, rate: f64) -> f64 {
    (-1.0 / (seconds * rate).max(1.0)).exp()
}

// --- Loudness (BS.1770, as `editor::loudness`) -----------------------------

/// The two K-weighting stages (high shelf, high-pass) for a sample rate.
fn k_weighting(rate: f64) -> [Biquad; 2] {
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347f64, 0.7071752369554196);
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let shelf = Biquad::from([vh + vb * k / q + k * k, 2.0 * (k * k - vh), vh - vb * k / q + k * k], 1.0 + k / q + k * k, [
        2.0 * (k * k - 1.0),
        1.0 - k / q + k * k,
    ]);
    let (f1, q1) = (38.13547087602444, 0.5003270373238773);
    let k1 = (PI * f1 / rate).tan();
    let a0 = 1.0 + k1 / q1 + k1 * k1;
    // BS.1770's high-pass has an unscaled numerator (1, −2, 1).
    let high_pass = Biquad { b: [1.0, -2.0, 1.0], a: [2.0 * (k1 * k1 - 1.0) / a0, (1.0 - k1 / q1 + k1 * k1) / a0], state: [0.0; 4] };
    [shelf, high_pass]
}

/// Gated integrated loudness in LUFS (400 ms blocks every 100 ms, absolute
/// gate at −70, relative gate at −10 LU); `-inf` when nothing passes.
pub fn integrated_loudness<T: Copy + Into<f64>>(channels: &[Vec<T>], rate: f64) -> f64 {
    let length = channels.iter().map(Vec::len).min().unwrap_or(0);
    let window = (0.4 * rate).round() as usize;
    let hop = ((0.1 * rate).round() as usize).max(1);
    if window == 0 || length < window {
        return f64::NEG_INFINITY;
    }
    let prefixes: Vec<Vec<f64>> = channels
        .iter()
        .map(|channel| {
            let mut stages = k_weighting(rate);
            let mut sums = Vec::with_capacity(length + 1);
            let mut running = 0.0;
            sums.push(0.0);
            for sample in &channel[..length] {
                let weighted = stages.iter_mut().fold((*sample).into(), |value, stage| stage.tick(value));
                running += weighted * weighted;
                sums.push(running);
            }
            sums
        })
        .collect();
    let lufs = |mean_square: f64| if mean_square > 0.0 { -0.691 + 10.0 * mean_square.log10() } else { f64::NEG_INFINITY };
    let mut blocks = Vec::new();
    let mut start = 0;
    while start + window <= length {
        blocks.push(lufs(prefixes.iter().map(|sums| (sums[start + window] - sums[start]) / window as f64).sum()));
        start += hop;
    }
    let mean = |values: &[f64]| {
        if values.is_empty() {
            return f64::NEG_INFINITY;
        }
        lufs(values.iter().map(|value| 10f64.powf((value + 0.691) / 10.0)).sum::<f64>() / values.len() as f64)
    };
    let above: Vec<f64> = blocks.into_iter().filter(|value| *value > -70.0).collect();
    if above.is_empty() {
        return f64::NEG_INFINITY;
    }
    let relative = mean(&above) - 10.0;
    let gated: Vec<f64> = above.into_iter().filter(|value| *value > relative).collect();
    mean(&gated)
}

/// Scales the planes so their integrated loudness is `target` LUFS (no-op for silence).
fn apply_gain_to(planes: &mut [Vec<f64>], rate: f64, target: f64) {
    let loudness = integrated_loudness(planes, rate);
    if !loudness.is_finite() {
        return;
    }
    let gain = 10f64.powf((target - loudness) / 20.0);
    for sample in planes.iter_mut().flatten() {
        *sample *= gain;
    }
}

// --- Noise reduction ---------------------------------------------------------

/// The settings of `apps/web/src/denoise/spectral.ts` (`DENOISE_DEFAULTS`
/// there is the reduction tool's; enhance uses a gentler floor).
pub struct DenoiseSettings {
    pub reduction_db: f64,
    pub over_subtraction: f64,
    pub noise_percentile: f64,
    pub power_smoothing: f64,
}

impl DenoiseSettings {
    pub const ENHANCE: Self = Self { reduction_db: 10.0, over_subtraction: 3.0, noise_percentile: 0.3, power_smoothing: 0.85 };
}

const DENOISE_FRAME: usize = 1024;
const DENOISE_HOP: usize = DENOISE_FRAME / 4;
/// At most this many frames feed the noise profile (evenly spread), so long
/// clips do not hold every frame's spectrum.
const PROFILE_FRAMES: usize = 4000;

/// The 50 ms window levels (dBFS of the channels' mean square), loudest to quietest not sorted.
fn window_levels(planes: &[Vec<f64>], rate: f64) -> Vec<f64> {
    let length = planes.iter().map(Vec::len).min().unwrap_or(0);
    let window = ((0.05 * rate) as usize).max(1);
    (0..length / window)
        .map(|index| {
            let range = index * window..(index + 1) * window;
            let mean_square = planes.iter().map(|plane| plane[range.clone()].iter().map(|s| s * s).sum::<f64>()).sum::<f64>()
                / (window * planes.len()) as f64;
            10.0 * (mean_square + 1e-20).log10()
        })
        .collect()
}

/// True when the quietest tenth of the clip sits at least 15 dB under its
/// loudest tenth and above digital silence: pauses with steady background
/// in them, which is what the noise profile is learned from.
fn has_steady_pauses(planes: &[Vec<f64>], rate: f64) -> bool {
    let mut levels = window_levels(planes, rate);
    if levels.len() < 20 {
        return false;
    }
    levels.sort_by(|a, b| a.total_cmp(b));
    let quiet = levels[levels.len() / 10];
    let loud = levels[levels.len() * 9 / 10];
    loud - quiet >= 15.0 && quiet > -90.0
}

/// Spectral subtraction on one channel (the web noise reduction tool's
/// method): per-bin noise is a low percentile of the bin's magnitude over
/// the clip, each frame's gain removes that noise power from the smoothed
/// power down to a floor, gains are median-smoothed across neighbouring bins.
pub fn denoise(samples: &[f64], settings: &DenoiseSettings) -> Vec<f64> {
    if samples.is_empty() {
        return Vec::new();
    }
    let window: Vec<f64> = (0..DENOISE_FRAME).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / DENOISE_FRAME as f64).cos()).collect();
    let frames = samples.len().div_ceil(DENOISE_HOP) + 3;
    let bins = DENOISE_FRAME / 2 + 1;
    let (mut re, mut im) = (vec![0f64; DENOISE_FRAME], vec![0f64; DENOISE_FRAME]);
    let load = |frame: usize, re: &mut [f64], im: &mut [f64]| {
        let offset = (frame * DENOISE_HOP) as isize - 3 * DENOISE_HOP as isize;
        for i in 0..DENOISE_FRAME {
            let index = offset + i as isize;
            re[i] = if index >= 0 && (index as usize) < samples.len() { samples[index as usize] * window[i] } else { 0.0 };
            im[i] = 0.0;
        }
        fft(re, im, false);
    };
    // Pass 1: the noise profile from (at most PROFILE_FRAMES) frames.
    let step = frames.div_ceil(PROFILE_FRAMES).max(1);
    let mut columns: Vec<Vec<f32>> = vec![Vec::with_capacity(frames / step + 1); bins];
    for frame in (0..frames).step_by(step) {
        load(frame, &mut re, &mut im);
        for (bin, column) in columns.iter_mut().enumerate() {
            column.push(re[bin].hypot(im[bin]) as f32);
        }
    }
    let percentile = settings.noise_percentile.clamp(0.0, 1.0);
    let profile: Vec<f64> = columns
        .iter_mut()
        .map(|column| {
            column.sort_by(|a, b| a.total_cmp(b));
            column[((column.len() as f64 * percentile) as usize).min(column.len() - 1)] as f64
        })
        .collect();
    drop(columns);
    // Pass 2: the gains, applied and overlap-added.
    let floor = 10f64.powf(-settings.reduction_db.max(0.0) / 20.0);
    let over = settings.over_subtraction.max(0.0);
    let keep = settings.power_smoothing.clamp(0.0, 0.99);
    let mut out = vec![0f64; samples.len()];
    let mut weight = vec![0f64; samples.len()];
    let (mut gain, mut smoothed, mut steady) = (vec![0f64; bins], vec![0f64; bins], vec![0f64; bins]);
    for frame in 0..frames {
        load(frame, &mut re, &mut im);
        for bin in 0..bins {
            let power = re[bin] * re[bin] + im[bin] * im[bin];
            steady[bin] = if frame == 0 { power } else { keep * steady[bin] + (1.0 - keep) * power };
            let noise = profile[bin] * profile[bin] * over;
            gain[bin] = if steady[bin] > 0.0 { floor.max(((steady[bin] - noise).max(0.0) / steady[bin]).sqrt()) } else { floor };
        }
        for bin in 0..bins {
            let (a, b, c) = (gain[bin.saturating_sub(1)], gain[bin], gain[(bin + 1).min(bins - 1)]);
            smoothed[bin] = a + b + c - a.max(b).max(c) - a.min(b).min(c);
        }
        for bin in 0..bins {
            re[bin] *= smoothed[bin];
            im[bin] *= smoothed[bin];
            if bin > 0 && bin < DENOISE_FRAME / 2 {
                re[DENOISE_FRAME - bin] = re[bin];
                im[DENOISE_FRAME - bin] = -im[bin];
            }
        }
        fft(&mut re, &mut im, true);
        let offset = (frame * DENOISE_HOP) as isize - 3 * DENOISE_HOP as isize;
        for i in 0..DENOISE_FRAME {
            let index = offset + i as isize;
            if index < 0 || index as usize >= samples.len() {
                continue;
            }
            out[index as usize] += re[i] / DENOISE_FRAME as f64 * window[i];
            weight[index as usize] += window[i] * window[i];
        }
    }
    out.iter().zip(&weight).map(|(value, weight)| if *weight > 1e-9 { value / weight } else { 0.0 }).collect()
}

/// In-place radix-2 FFT in f64; `inverse` runs without the 1/N scale.
fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
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
        let angle = sign * 2.0 * PI / size as f64;
        let (step_re, step_im) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(size) {
            let (mut w_re, mut w_im) = (1f64, 0f64);
            for k in 0..size / 2 {
                let (a, b) = (start + k, start + k + size / 2);
                let (b_re, b_im) = (re[b] * w_re - im[b] * w_im, re[b] * w_im + im[b] * w_re);
                re[b] = re[a] - b_re;
                im[b] = im[a] - b_im;
                re[a] += b_re;
                im[a] += b_im;
                (w_re, w_im) = (w_re * step_re - w_im * step_im, w_re * step_im + w_im * step_re);
            }
        }
        size <<= 1;
    }
}

// --- Dynamics ----------------------------------------------------------------

/// A linear-phase windowed-sinc (Blackman) high-pass, about 1.3 ms long.
fn fir_high_pass(cutoff: f64, rate: f64) -> Vec<f64> {
    let half = ((32.0 * rate / 48_000.0).round() as usize).max(8);
    let length = 2 * half + 1;
    let fc = (cutoff / rate).min(0.45);
    let mut taps: Vec<f64> = (0..length)
        .map(|i| {
            let n = i as f64 - half as f64;
            let sinc = if n == 0.0 { 2.0 * fc } else { (2.0 * PI * fc * n).sin() / (PI * n) };
            let x = i as f64 / (length - 1) as f64;
            sinc * (0.42 - 0.5 * (2.0 * PI * x).cos() + 0.08 * (4.0 * PI * x).cos())
        })
        .collect();
    // Unity gain at DC for the low-pass, then flip it into a high-pass.
    let sum: f64 = taps.iter().sum();
    for tap in taps.iter_mut() {
        *tap = -*tap / sum;
    }
    taps[half] += 1.0;
    taps
}

/// `samples` through symmetric `taps` with the delay removed (offline, so
/// the band lines up with the input sample for sample).
fn convolve_centred(samples: &[f64], taps: &[f64]) -> Vec<f64> {
    let half = taps.len() / 2;
    let mut padded = vec![0f64; samples.len() + 2 * half];
    padded[half..half + samples.len()].copy_from_slice(samples);
    padded.windows(taps.len()).map(|window| window.iter().zip(taps).map(|(sample, tap)| sample * tap).sum()).collect()
}

/// Turns the 5 kHz-and-up band down while it carries most of the energy:
/// the band's gain is `0.45 × full / band` of the envelopes (1 ms attack,
/// 60 ms release), between 0.35 (−9 dB) and 1. The band is split off with
/// a linear-phase high-pass lined up with the input, and the output is
/// `x + (gain − 1) × band`, so a gain of 1 gives the input back exactly.
fn de_ess(planes: &mut [Vec<f64>], rate: f64) {
    const SHARE: f64 = 0.45;
    let length = planes.iter().map(Vec::len).min().unwrap_or(0);
    let taps = fir_high_pass(5000.0, rate);
    let bands: Vec<Vec<f64>> = planes.iter().map(|plane| convolve_centred(plane, &taps)).collect();
    let (attack, release) = (coefficient(0.001, rate), coefficient(0.06, rate));
    let follow = |envelope: f64, level: f64| {
        let k = if level > envelope { attack } else { release };
        k * envelope + (1.0 - k) * level
    };
    let (mut full, mut high) = (0f64, 0f64);
    for i in 0..length {
        let full_level = planes.iter().map(|plane| plane[i].abs()).fold(0.0, f64::max);
        let high_level = bands.iter().map(|band| band[i].abs()).fold(0.0, f64::max);
        full = follow(full, full_level);
        high = follow(high, high_level);
        let gain = if high > 1e-9 { (SHARE * full / high).clamp(0.35, 1.0) } else { 1.0 };
        if gain < 1.0 {
            for (plane, band) in planes.iter_mut().zip(&bands) {
                plane[i] += (gain - 1.0) * band[i];
            }
        }
    }
}

/// 3:1 above −26 dBFS with a 6 dB soft knee, from a linked peak envelope
/// (5 ms attack, 120 ms release); no make-up gain (the loudness stage after
/// it sets the level). Where the short-term RMS (10 ms) is under −48 dBFS
/// a downward expander (2 dB down per dB under, at most −12 dB) keeps the
/// pauses from coming up with the speech's gain.
fn compress(planes: &mut [Vec<f64>], rate: f64) {
    const THRESHOLD_DB: f64 = -26.0;
    const RATIO: f64 = 3.0;
    const KNEE_DB: f64 = 6.0;
    const EXPANDER_DB: f64 = -48.0;
    let length = planes.iter().map(Vec::len).min().unwrap_or(0);
    let (attack, release) = (coefficient(0.005, rate), coefficient(0.12, rate));
    let smoothing = coefficient(0.01, rate);
    let (mut envelope, mut power) = (0f64, 0f64);
    for i in 0..length {
        let level = planes.iter().map(|plane| plane[i].abs()).fold(0.0, f64::max);
        let k = if level > envelope { attack } else { release };
        envelope = k * envelope + (1.0 - k) * level;
        let square = planes.iter().map(|plane| plane[i] * plane[i]).sum::<f64>() / planes.len() as f64;
        power = smoothing * power + (1.0 - smoothing) * square;
        let peak_db = 20.0 * (envelope + 1e-12).log10();
        let rms_db = 10.0 * (power + 1e-24).log10();
        let reduction = compressor_reduction_db(peak_db, THRESHOLD_DB, RATIO, KNEE_DB) + (2.0 * (rms_db - EXPANDER_DB)).clamp(-12.0, 0.0);
        if reduction < 0.0 {
            let gain = 10f64.powf(reduction / 20.0);
            for plane in planes.iter_mut() {
                plane[i] *= gain;
            }
        }
    }
}

/// The gain change (≤ 0 dB) of a soft-knee compressor for an input level.
pub fn compressor_reduction_db(level_db: f64, threshold_db: f64, ratio: f64, knee_db: f64) -> f64 {
    let over = level_db - threshold_db;
    let slope = 1.0 / ratio - 1.0;
    if 2.0 * over <= -knee_db {
        0.0
    } else if 2.0 * over >= knee_db {
        slope * over
    } else {
        slope * (over + knee_db / 2.0).powi(2) / (2.0 * knee_db)
    }
}

/// Holds the sample peak at −1 dBFS: an instant attack and a 50 ms release
/// on the gain, linked across channels.
fn limit(planes: &mut [Vec<f64>], rate: f64) {
    let length = planes.iter().map(Vec::len).min().unwrap_or(0);
    let release = coefficient(0.05, rate);
    let mut gain = 1f64;
    for i in 0..length {
        let level = planes.iter().map(|plane| plane[i].abs()).fold(0.0, f64::max);
        let needed = if level > PEAK_CEILING { PEAK_CEILING / level } else { 1.0 };
        gain = if needed < gain { needed } else { release * gain + (1.0 - release) * needed };
        for plane in planes.iter_mut() {
            plane[i] *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn tone(frequency: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(RATE * seconds) as usize).map(|i| (2.0 * std::f32::consts::PI * frequency * i as f32 / RATE).sin() * amplitude).collect()
    }

    fn rms(samples: &[f64]) -> f64 {
        (samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64).sqrt()
    }

    /// A deterministic noise source (xorshift), uniform in ±amplitude.
    fn noise(length: usize, amplitude: f32, seed: u32) -> Vec<f32> {
        let mut state = seed.max(1);
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
            })
            .collect()
    }

    /// "Speech": 0.6 s bursts of a voiced tone mix every second, with pauses.
    fn speech_with_hiss() -> Vec<f32> {
        let length = (RATE * 6.0) as usize;
        let hiss = noise(length, 0.01, 7);
        (0..length)
            .map(|i| {
                let t = i as f32 / RATE;
                let voiced = if t % 1.0 < 0.6 {
                    let phase = 2.0 * std::f32::consts::PI * t;
                    0.15 * (phase * 180.0).sin() + 0.08 * (phase * 360.0).sin() + 0.05 * (phase * 1200.0).sin() + 0.03 * (phase * 2800.0).sin()
                } else {
                    0.0
                };
                voiced + hiss[i]
            })
            .collect()
    }

    #[test]
    fn loudness_matches_the_editor_meter() {
        let channels = vec![tone(1000.0, 3.0, 0.25), tone(500.0, 3.0, 0.1)];
        let ours = integrated_loudness(&channels, RATE as f64);
        let editor = editor::measure_loudness(&channels, RATE as u32).integrated_lufs;
        assert!((ours - editor).abs() < 0.01, "{ours} vs {editor}");
    }

    #[test]
    fn enhance_reaches_the_target_and_keeps_the_length() {
        for amplitude in [0.02f32, 0.5] {
            let mut channels = vec![speech_with_hiss().iter().map(|s| s * amplitude * 2.0).collect::<Vec<f32>>()];
            let length = channels[0].len();
            let report = enhance_voice(&mut channels, RATE);
            assert_eq!(channels[0].len(), length);
            assert!((report.output_lufs - ENHANCE_TARGET_LUFS).abs() < 0.7, "{amplitude}: {report:?}");
            assert!(channels[0].iter().all(|s| s.abs() <= 0.8913), "{amplitude}: peak over −1 dBFS");
            assert!(report.noise_reduced, "{amplitude}: the pauses should have been cleaned");
        }
    }

    #[test]
    fn enhance_quiets_the_pauses_more_than_the_speech() {
        let input = speech_with_hiss();
        let mut channels = vec![input.clone()];
        enhance_voice(&mut channels, RATE);
        let part = |samples: &[f32], from: f32, to: f32| {
            rms(&samples[(from * RATE) as usize..(to * RATE) as usize].iter().map(|s| *s as f64).collect::<Vec<_>>())
        };
        // Speech-to-pause ratio over the middle seconds, before and after.
        let before = part(&input, 2.1, 2.5) / part(&input, 2.7, 2.95);
        let after = part(&channels[0], 2.1, 2.5) / part(&channels[0], 2.7, 2.95);
        assert!(after > 1.8 * before, "pause got {before:.1}× → {after:.1}× quieter than speech");
    }

    #[test]
    fn clips_without_pauses_skip_noise_reduction() {
        let mut channels = vec![tone(300.0, 3.0, 0.3)];
        let report = enhance_voice(&mut channels, RATE);
        assert!(!report.noise_reduced);
        assert!((report.output_lufs - ENHANCE_TARGET_LUFS).abs() < 0.5, "{report:?}");
    }

    #[test]
    fn de_esser_turns_down_sibilance_only() {
        let measure = |frequency: f32| {
            let mut planes = vec![tone(frequency, 1.0, 0.3).iter().map(|s| *s as f64).collect::<Vec<f64>>()];
            let before = rms(&planes[0][24_000..]);
            de_ess(&mut planes, RATE as f64);
            rms(&planes[0][24_000..]) / before
        };
        let voice = measure(1000.0);
        let hiss = measure(7000.0);
        assert!((voice - 1.0).abs() < 0.02, "1 kHz changed by {voice}");
        assert!(hiss < 0.6, "7 kHz kept {hiss}");
    }

    #[test]
    fn compressor_curve() {
        assert_eq!(compressor_reduction_db(-40.0, -26.0, 3.0, 6.0), 0.0);
        assert!((compressor_reduction_db(-14.0, -26.0, 3.0, 6.0) + 8.0).abs() < 1e-9);
        let knee = compressor_reduction_db(-26.0, -26.0, 3.0, 6.0);
        assert!(knee < 0.0 && knee > -1.0, "{knee}");
    }

    #[test]
    fn silence_stays_silent() {
        let mut channels = vec![vec![0f32; 48_000], vec![0f32; 48_000]];
        let report = enhance_voice(&mut channels, RATE);
        assert!(channels.iter().flatten().all(|s| *s == 0.0));
        assert!(!report.input_lufs.is_finite() && !report.noise_reduced);
    }
}

#[cfg(test)]
mod speed {
    #[test]
    #[ignore]
    fn one_minute_stereo() {
        let length = 48_000 * 60;
        let mut channels: Vec<Vec<f32>> = (0..2).map(|c| (0..length).map(|i| ((i * (c + 3)) as f32 * 0.01).sin() * if i % 48_000 < 30_000 { 0.3 } else { 0.01 }).collect()).collect();
        let start = std::time::Instant::now();
        let report = super::enhance_voice(&mut channels, 48_000.0);
        println!("one minute stereo: {:?} {report:?}", start.elapsed());
    }
}

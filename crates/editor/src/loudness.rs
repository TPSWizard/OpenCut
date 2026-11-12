//! Loudness measurement after ITU-R BS.1770-4 and EBU R128: K-weighted,
//! gated integrated loudness, loudness range (EBU Tech 3342), true peak
//! (4× oversampled) and the loudest momentary (400 ms) and short-term
//! (3 s) windows, plus the gain that normalises a clip to a target. The
//! web app's `src/loudness/r128.ts` is the same arithmetic.
//!
//! Channels are weighted equally (1.0, as for left, right and centre);
//! surround weights are not applied.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessReport {
    /// Gated integrated loudness in LUFS; `-inf` when nothing passes the gate.
    pub integrated_lufs: f64,
    /// Loudness range in LU.
    pub loudness_range_lu: f64,
    /// The highest true peak in dBTP; `-inf` for silence.
    pub true_peak_dbtp: f64,
    pub max_momentary_lufs: f64,
    pub max_short_term_lufs: f64,
    pub seconds: f64,
    pub sample_rate: u32,
    pub channels: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessTarget {
    pub id: &'static str,
    pub label: &'static str,
    pub lufs: f64,
    pub true_peak_dbtp: f64,
}

/// Common delivery targets; the first is the default.
pub const LOUDNESS_TARGETS: &[LoudnessTarget] = &[
    LoudnessTarget { id: "streaming", label: "Streaming −14", lufs: -14.0, true_peak_dbtp: -1.0 },
    LoudnessTarget { id: "podcast", label: "Podcast −16", lufs: -16.0, true_peak_dbtp: -1.0 },
    LoudnessTarget { id: "broadcast", label: "Broadcast −23", lufs: -23.0, true_peak_dbtp: -1.0 },
    LoudnessTarget { id: "loud", label: "Loud social −11", lufs: -11.0, true_peak_dbtp: -1.0 },
];

pub const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;
const RANGE_RELATIVE_GATE_LU: f64 = -20.0;
const MOMENTARY_SECONDS: f64 = 0.4;
const SHORT_TERM_SECONDS: f64 = 3.0;
const HOP_SECONDS: f64 = 0.1;
const LOUDNESS_OFFSET: f64 = -0.691;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

/// The two K-weighting stages (high shelf, high-pass) for a sample rate.
fn k_weighting(sample_rate: f64) -> [Biquad; 2] {
    let f0 = 1681.974450955533;
    let gain_db: f64 = 3.999843853973347;
    let q = 0.7071752369554196;
    let k = (std::f64::consts::PI * f0 / sample_rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };
    let f1 = 38.13547087602444;
    let q1 = 0.5003270373238773;
    let k1 = (std::f64::consts::PI * f1 / sample_rate).tan();
    let a01 = 1.0 + k1 / q1 + k1 * k1;
    let high_pass = Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k1 * k1 - 1.0) / a01,
        a2: (1.0 - k1 / q1 + k1 * k1) / a01,
    };
    [shelf, high_pass]
}

fn filter_in_place(samples: &mut [f64], stages: &[Biquad]) {
    for stage in stages {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for sample in samples.iter_mut() {
            let x0 = *sample;
            let y0 = stage.b0 * x0 + stage.b1 * x1 + stage.b2 * x2 - stage.a1 * y1 - stage.a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            *sample = y0;
        }
    }
}

fn lufs(mean_square: f64) -> f64 {
    if mean_square > 0.0 { LOUDNESS_OFFSET + 10.0 * mean_square.log10() } else { f64::NEG_INFINITY }
}

fn mean_of_powers(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }
    let sum: f64 = values.iter().map(|value| 10f64.powf((value - LOUDNESS_OFFSET) / 10.0)).sum();
    lufs(sum / values.len() as f64)
}

/// The loudness of each window, every `HOP_SECONDS`, from the summed
/// K-weighted mean squares of the channels.
fn window_loudness(weighted: &[Vec<f64>], sample_rate: f64, window_seconds: f64) -> Vec<f64> {
    let length = weighted.first().map(|channel| channel.len()).unwrap_or(0);
    let window = (window_seconds * sample_rate).round() as usize;
    let hop = ((HOP_SECONDS * sample_rate).round() as usize).max(1);
    if window == 0 || length < window {
        return Vec::new();
    }
    let prefix: Vec<Vec<f64>> = weighted
        .iter()
        .map(|channel| {
            let mut sums = Vec::with_capacity(channel.len() + 1);
            sums.push(0.0);
            let mut running = 0.0;
            for sample in channel {
                running += sample * sample;
                sums.push(running);
            }
            sums
        })
        .collect();
    let mut values = Vec::new();
    let mut start = 0;
    while start + window <= length {
        let mean_square: f64 = prefix.iter().map(|sums| (sums[start + window] - sums[start]) / window as f64).sum();
        values.push(lufs(mean_square));
        start += hop;
    }
    values
}

/// Gated integrated loudness of momentary values (absolute, then relative gate).
pub fn gated_loudness(momentary: &[f64]) -> f64 {
    let above: Vec<f64> = momentary.iter().copied().filter(|value| *value > ABSOLUTE_GATE_LUFS).collect();
    if above.is_empty() {
        return f64::NEG_INFINITY;
    }
    let relative = mean_of_powers(&above) + RELATIVE_GATE_LU;
    let gated: Vec<f64> = above.into_iter().filter(|value| *value > relative).collect();
    mean_of_powers(&gated)
}

/// Loudness range of short-term values (EBU Tech 3342).
pub fn loudness_range(short_term: &[f64]) -> f64 {
    let above: Vec<f64> = short_term.iter().copied().filter(|value| *value > ABSOLUTE_GATE_LUFS).collect();
    if above.is_empty() {
        return 0.0;
    }
    let relative = mean_of_powers(&above) + RANGE_RELATIVE_GATE_LU;
    let mut gated: Vec<f64> = above.into_iter().filter(|value| *value > relative).collect();
    if gated.len() < 2 {
        return 0.0;
    }
    gated.sort_by(|a, b| a.total_cmp(b));
    let at = |fraction: f64| gated[(((gated.len() - 1) as f64 * fraction).round() as usize).min(gated.len() - 1)];
    (at(0.95) - at(0.1)).max(0.0)
}

/// Polyphase coefficients for `factor`× oversampling: a windowed-sinc
/// low-pass of `taps` taps per phase (Blackman window), scaled so a
/// constant passes unchanged.
pub fn oversampling_phases(factor: usize, taps: usize) -> Vec<Vec<f64>> {
    let total = taps * factor;
    let centre = (total as f64 - 1.0) / 2.0;
    let kernel: Vec<f64> = (0..total)
        .map(|index| {
            let x = (index as f64 - centre) / factor as f64;
            let sinc = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let n = index as f64;
            let m = total as f64 - 1.0;
            let window = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * n / m).cos() + 0.08 * (4.0 * std::f64::consts::PI * n / m).cos();
            sinc * window
        })
        .collect();
    (0..factor)
        .map(|phase| {
            let coefficients: Vec<f64> = (0..taps).map(|tap| kernel[tap * factor + phase]).collect();
            let sum: f64 = coefficients.iter().sum();
            coefficients.into_iter().map(|value| value / sum).collect()
        })
        .collect()
}

/// The highest absolute sample after oversampling, in dBTP.
pub fn true_peak(channels: &[Vec<f32>], sample_rate: u32) -> f64 {
    let factor = if sample_rate >= 96_000 { 2 } else { 4 };
    let phases = oversampling_phases(factor, 12);
    let mut peak = 0.0f64;
    for channel in channels {
        for index in 0..channel.len() {
            peak = peak.max((channel[index] as f64).abs());
            for coefficients in &phases {
                let half = coefficients.len() / 2;
                let mut value = 0.0;
                for (tap, coefficient) in coefficients.iter().enumerate() {
                    let at = index as i64 - tap as i64 + half as i64;
                    if at >= 0 && (at as usize) < channel.len() {
                        value += coefficient * channel[at as usize] as f64;
                    }
                }
                peak = peak.max(value.abs());
            }
        }
    }
    if peak > 0.0 { 20.0 * peak.log10() } else { f64::NEG_INFINITY }
}

/// Measures `channels` (one buffer per channel, equal lengths).
pub fn measure_loudness(channels: &[Vec<f32>], sample_rate: u32) -> LoudnessReport {
    let length = channels.first().map(|channel| channel.len()).unwrap_or(0);
    let stages = k_weighting(sample_rate as f64);
    let weighted: Vec<Vec<f64>> = channels
        .iter()
        .map(|channel| {
            let mut copy: Vec<f64> = channel.iter().map(|sample| *sample as f64).collect();
            filter_in_place(&mut copy, &stages);
            copy
        })
        .collect();
    let momentary = window_loudness(&weighted, sample_rate as f64, MOMENTARY_SECONDS);
    let short_term = window_loudness(&weighted, sample_rate as f64, SHORT_TERM_SECONDS);
    let max = |values: &[f64]| values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    LoudnessReport {
        integrated_lufs: gated_loudness(&momentary),
        loudness_range_lu: loudness_range(&short_term),
        true_peak_dbtp: true_peak(channels, sample_rate),
        max_momentary_lufs: max(&momentary),
        max_short_term_lufs: max(&short_term),
        seconds: length as f64 / sample_rate as f64,
        sample_rate,
        channels: channels.len(),
    }
}

/// Splits interleaved samples into channels.
pub fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let channels = channels.max(1);
    let mut out: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(samples.len() / channels)).collect();
    for (index, sample) in samples.iter().enumerate() {
        out[index % channels].push(*sample);
    }
    out
}

/// The gain in dB that brings the measured audio to `target_lufs`, held
/// back so the true peak stays under `true_peak_dbtp`; `(gain, limited)`,
/// or none when nothing was measured.
pub fn normalization_gain(report: &LoudnessReport, target_lufs: f64, true_peak_dbtp: f64) -> Option<(f64, bool)> {
    if !report.integrated_lufs.is_finite() {
        return None;
    }
    let wanted = target_lufs - report.integrated_lufs;
    let headroom = if report.true_peak_dbtp.is_finite() { true_peak_dbtp - report.true_peak_dbtp } else { f64::INFINITY };
    if wanted > headroom { Some((headroom, true)) } else { Some((wanted, false)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(seconds: f64, amplitude: f64, sample_rate: u32) -> Vec<f32> {
        let count = (seconds * sample_rate as f64).round() as usize;
        (0..count)
            .map(|index| (amplitude * (2.0 * std::f64::consts::PI * 1000.0 * index as f64 / sample_rate as f64).sin()) as f32)
            .collect()
    }

    fn db(amplitude: f64) -> f64 {
        20.0 * amplitude.log10()
    }

    #[test]
    fn k_weighting_reproduces_the_standard_coefficients() {
        let [shelf, high_pass] = k_weighting(48_000.0);
        assert!((shelf.b0 - 1.53512485958697).abs() < 1e-6);
        assert!((shelf.b1 + 2.69169618940638).abs() < 1e-6);
        assert!((shelf.b2 - 1.19839281085285).abs() < 1e-6);
        assert!((shelf.a1 + 1.69065929318241).abs() < 1e-6);
        assert!((shelf.a2 - 0.73248077421585).abs() < 1e-6);
        assert!((high_pass.a1 + 1.99004745483398).abs() < 1e-6);
        assert!((high_pass.a2 - 0.99007225036621).abs() < 1e-6);
    }

    #[test]
    fn a_sine_reads_three_db_below_its_level() {
        let amplitude = 0.0125;
        let expected = db(amplitude) - 3.01;
        let mono = measure_loudness(&[sine(5.0, amplitude, RATE)], RATE);
        assert!((mono.integrated_lufs - expected).abs() < 0.1, "{}", mono.integrated_lufs);
        assert!(mono.loudness_range_lu < 0.2);
        assert!((mono.true_peak_dbtp - db(amplitude)).abs() < 0.2, "{}", mono.true_peak_dbtp);
        assert!((mono.max_momentary_lufs - mono.integrated_lufs).abs() < 0.1);
        assert_eq!(mono.seconds, 5.0);
        let stereo = measure_loudness(&[sine(5.0, amplitude, RATE), sine(5.0, amplitude, RATE)], RATE);
        assert!((stereo.integrated_lufs - (expected + 3.01)).abs() < 0.1, "{}", stereo.integrated_lufs);
        let at441 = measure_loudness(&[sine(5.0, amplitude, 44_100)], 44_100);
        assert!((at441.integrated_lufs - expected).abs() < 0.15, "{}", at441.integrated_lufs);
    }

    #[test]
    fn quiet_passages_and_silence_are_gated() {
        let mut signal = sine(2.0, 0.1, RATE);
        signal.extend(sine(2.0, 0.001, RATE));
        signal.extend(sine(2.0, 0.1, RATE));
        let report = measure_loudness(&[signal], RATE);
        let loud = db(0.1) - 3.01;
        assert!(report.integrated_lufs > loud - 0.6 && report.integrated_lufs < loud + 0.1, "{}", report.integrated_lufs);
        assert!(report.loudness_range_lu < 4.0, "{}", report.loudness_range_lu);
        let silence = measure_loudness(&[vec![0.0; RATE as usize]], RATE);
        assert_eq!(silence.integrated_lufs, f64::NEG_INFINITY);
        assert_eq!(silence.true_peak_dbtp, f64::NEG_INFINITY);
        assert_eq!(silence.loudness_range_lu, 0.0);
        assert!(normalization_gain(&silence, -14.0, -1.0).is_none());
    }

    #[test]
    fn stepping_levels_show_a_range_and_peaks_are_found_between_samples() {
        let mut signal = Vec::new();
        for amplitude in [0.2, 0.02, 0.2, 0.02] {
            signal.extend(sine(4.0, amplitude, RATE));
        }
        let report = measure_loudness(&[signal], RATE);
        assert!(report.loudness_range_lu > 10.0 && report.loudness_range_lu < 21.0, "{}", report.loudness_range_lu);

        let quarter: Vec<f32> = (0..4800)
            .map(|index| {
                (0.5 * (2.0 * std::f64::consts::PI * (RATE as f64 / 4.0) * index as f64 / RATE as f64 + std::f64::consts::FRAC_PI_4).sin())
                    as f32
            })
            .collect();
        let sampled = quarter.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(sampled < 0.36);
        let peak = true_peak(&[quarter], RATE);
        assert!((peak - db(0.5)).abs() < 0.3, "{peak}");
        for phase in oversampling_phases(4, 12) {
            assert!((phase.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn normalisation_aims_at_the_target_within_the_ceiling() {
        let report = measure_loudness(&[sine(3.0, 0.05, RATE)], RATE);
        let (gain, limited) = normalization_gain(&report, -23.0, -1.0).unwrap();
        assert!(!limited);
        assert!((gain - (-23.0 - report.integrated_lufs)).abs() < 1e-9);
        let (gain, limited) = normalization_gain(&report, -2.0, -1.0).unwrap();
        assert!(limited);
        assert!((gain - (-1.0 - report.true_peak_dbtp)).abs() < 1e-9);
        assert_eq!(deinterleave(&[1.0, 2.0, 3.0, 4.0], 2), vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
        assert_eq!(loudness_range(&[-20.0, -20.0, -20.0]), 0.0);
    }
}

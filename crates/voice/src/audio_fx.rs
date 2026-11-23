//! Audio effects on a clip (row 72): a three-band equaliser, a compressor
//! and a limiter, applied in that order to all of a clip's channels. Kept
//! as clip params (`audio.eq.*`, `audio.comp.*`, `audio.limit.*`) and run
//! when the clip's audio is prepared, the same for preview and export:
//! the desktop's mixer and exporter call this directly, the web's through
//! `opencut-wasm`'s `applyAudioFx`.
//!
//! - Equaliser: a low shelf at 120 Hz, a bell at 1 kHz (Q 0.9) and a high
//!   shelf at 8 kHz, each ±12 dB (RBJ cookbook biquads).
//! - Compressor: feed-forward, the channels linked, a peak envelope (10 ms
//!   attack, 150 ms release), a 6 dB soft knee, and make-up gain of three
//!   quarters of the reduction a full-scale signal gets, so heavy settings
//!   do not leave the clip much quieter; with the limiter off, a safety
//!   limiter at -0.1 dBFS stops a sudden loud start clipping while the
//!   compressor reacts.
//! - Limiter: no peak over the ceiling (instant attack, 80 ms release).

use std::f64::consts::PI;

/// Equaliser band centres, Hz.
pub const EQ_LOW_HZ: f64 = 120.0;
pub const EQ_MID_HZ: f64 = 1000.0;
pub const EQ_HIGH_HZ: f64 = 8000.0;
/// Each band's range, ±dB.
pub const EQ_RANGE_DB: f64 = 12.0;

/// The effects on one clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFx {
    /// Equaliser gains in dB (0 leaves a band alone).
    pub low_db: f64,
    pub mid_db: f64,
    pub high_db: f64,
    /// `(threshold dB, ratio)` when the compressor is on.
    pub compressor: Option<(f64, f64)>,
    /// Ceiling in dB when the limiter is on.
    pub limiter: Option<f64>,
}

impl Default for AudioFx {
    fn default() -> Self {
        Self { low_db: 0.0, mid_db: 0.0, high_db: 0.0, compressor: None, limiter: None }
    }
}

/// The params the effects are kept in.
pub const AUDIO_FX_PARAMS: [&str; 8] = [
    "audio.eq.low",
    "audio.eq.mid",
    "audio.eq.high",
    "audio.comp.enabled",
    "audio.comp.threshold",
    "audio.comp.ratio",
    "audio.limit.enabled",
    "audio.limit.ceiling",
];
pub const DEFAULT_THRESHOLD_DB: f64 = -24.0;
pub const DEFAULT_RATIO: f64 = 4.0;
pub const DEFAULT_CEILING_DB: f64 = -1.0;

impl AudioFx {
    /// Whether the effects leave the audio exactly as it is.
    pub fn is_identity(&self) -> bool {
        self.low_db == 0.0 && self.mid_db == 0.0 && self.high_db == 0.0 && self.compressor.is_none() && self.limiter.is_none()
    }

    /// From clip params: `number(key)` and `flag(key)` read them (absent is `None`).
    pub fn from_params(number: impl Fn(&str) -> Option<f64>, flag: impl Fn(&str) -> Option<bool>) -> Self {
        let band = |key: &str| number(key).filter(|value| value.is_finite()).unwrap_or(0.0).clamp(-EQ_RANGE_DB, EQ_RANGE_DB);
        let compressor = flag("audio.comp.enabled").unwrap_or(false).then(|| {
            (
                number("audio.comp.threshold").filter(|v| v.is_finite()).unwrap_or(DEFAULT_THRESHOLD_DB).clamp(-60.0, 0.0),
                number("audio.comp.ratio").filter(|v| v.is_finite()).unwrap_or(DEFAULT_RATIO).clamp(1.0, 20.0),
            )
        });
        let limiter = flag("audio.limit.enabled")
            .unwrap_or(false)
            .then(|| number("audio.limit.ceiling").filter(|v| v.is_finite()).unwrap_or(DEFAULT_CEILING_DB).clamp(-12.0, 0.0));
        Self { low_db: band("audio.eq.low"), mid_db: band("audio.eq.mid"), high_db: band("audio.eq.high"), compressor, limiter }
    }

    /// A short stable text of the settings, for cache keys.
    pub fn key(&self) -> String {
        format!("{:.2}|{:.2}|{:.2}|{:?}|{:?}", self.low_db, self.mid_db, self.high_db, self.compressor, self.limiter)
    }
}

/// `(id, label, effects)` of the presets, in menu order.
pub fn audio_fx_presets() -> Vec<(&'static str, &'static str, AudioFx)> {
    let fx = |low_db, mid_db, high_db, compressor, limiter| AudioFx { low_db, mid_db, high_db, compressor, limiter };
    vec![
        ("none", "None", AudioFx::default()),
        ("voice-clarity", "Voice clarity", fx(-4.0, 2.0, 3.0, Some((-24.0, 3.0)), Some(-1.0))),
        ("podcast", "Podcast", fx(-2.0, 1.0, 2.0, Some((-30.0, 4.0)), Some(-1.0))),
        ("bass-boost", "Bass boost", fx(6.0, 0.0, 0.0, None, Some(-1.0))),
        ("bright", "Bright", fx(0.0, 0.0, 5.0, None, None)),
        ("warm", "Warm", fx(3.0, -1.0, -3.0, None, None)),
        ("loud", "Loud and even", fx(0.0, 0.0, 0.0, Some((-30.0, 8.0)), Some(-1.0))),
    ]
}

/// A biquad (RBJ cookbook), Direct Form I.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn normalised(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    fn peaking(frequency: f64, q: f64, gain_db: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * frequency / rate;
        let alpha = w.sin() / (2.0 * q);
        Self::normalised(1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a)
    }

    /// A shelf with slope 1: `high` for a high shelf, else a low shelf.
    fn shelf(frequency: f64, gain_db: f64, rate: f64, high: bool) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * frequency / rate;
        let (cos, sin) = (w.cos(), w.sin());
        let alpha = sin / 2.0 * 2f64.sqrt();
        let root = 2.0 * a.sqrt() * alpha;
        if high {
            Self::normalised(
                a * ((a + 1.0) + (a - 1.0) * cos + root),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - root),
                (a + 1.0) - (a - 1.0) * cos + root,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - root,
            )
        } else {
            Self::normalised(
                a * ((a + 1.0) - (a - 1.0) * cos + root),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - root),
                (a + 1.0) + (a - 1.0) * cos + root,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - root,
            )
        }
    }

    fn run(&self, samples: &mut [f32]) {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for sample in samples {
            let x = *sample as f64;
            let y = self.b0 * x + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            *sample = y as f32;
        }
    }
}

fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn gain_to_db(gain: f64) -> f64 {
    20.0 * gain.max(1e-9).log10()
}

const KNEE_DB: f64 = 6.0;

/// Gain reduction (dB, ≤ 0) for a level, with a soft knee.
fn reduction_db(level_db: f64, threshold_db: f64, ratio: f64) -> f64 {
    let over = level_db - threshold_db;
    let slope = 1.0 / ratio - 1.0;
    if over <= -KNEE_DB / 2.0 {
        0.0
    } else if over >= KNEE_DB / 2.0 {
        slope * over
    } else {
        slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
    }
}

fn compress(channels: &mut [Vec<f32>], rate: f64, threshold_db: f64, ratio: f64) {
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
    let attack = (-1.0 / (0.010 * rate)).exp();
    let release = (-1.0 / (0.150 * rate)).exp();
    // Make-up (auto gain): three quarters of what a full-scale signal loses,
    // so a full-scale peak still comes out a quarter of that under 0 dBFS.
    let makeup_db = -reduction_db(0.0, threshold_db, ratio) * 0.75;
    let mut envelope = 0.0f64;
    for index in 0..frames {
        let peak = channels.iter().map(|channel| (channel[index] as f64).abs()).fold(0.0, f64::max);
        let coefficient = if peak > envelope { attack } else { release };
        envelope = coefficient * envelope + (1.0 - coefficient) * peak;
        let gain = db_to_gain(reduction_db(gain_to_db(envelope), threshold_db, ratio) + makeup_db) as f32;
        for channel in channels.iter_mut() {
            channel[index] *= gain;
        }
    }
}

fn limit(channels: &mut [Vec<f32>], rate: f64, ceiling_db: f64) {
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
    let ceiling = db_to_gain(ceiling_db);
    let release = (-1.0 / (0.080 * rate)).exp();
    let mut gain = 1.0f64;
    for index in 0..frames {
        let peak = channels.iter().map(|channel| (channel[index] as f64).abs()).fold(0.0, f64::max);
        let wanted = if peak * gain > ceiling { ceiling / peak } else { 1.0 };
        gain = if wanted < gain { wanted } else { release * gain + (1.0 - release) * wanted };
        // Never over the ceiling, whatever the release left.
        let applied = gain.min(if peak > 0.0 { ceiling / peak } else { 1.0 }).min(1.0);
        for channel in channels.iter_mut() {
            channel[index] = (channel[index] as f64 * applied) as f32;
        }
    }
}

/// Runs the effects over a clip's channels (each the same length), in place.
pub fn apply_audio_fx(channels: &mut [Vec<f32>], sample_rate: f32, fx: &AudioFx) {
    let rate = sample_rate as f64;
    if rate <= 0.0 || fx.is_identity() {
        return;
    }
    let mut filters = Vec::new();
    if fx.low_db != 0.0 {
        filters.push(Biquad::shelf(EQ_LOW_HZ, fx.low_db, rate, false));
    }
    if fx.mid_db != 0.0 {
        filters.push(Biquad::peaking(EQ_MID_HZ, 0.9, fx.mid_db, rate));
    }
    if fx.high_db != 0.0 && EQ_HIGH_HZ < rate / 2.0 {
        filters.push(Biquad::shelf(EQ_HIGH_HZ, fx.high_db, rate, true));
    }
    for channel in channels.iter_mut() {
        for filter in &filters {
            filter.run(channel);
        }
    }
    if let Some((threshold, ratio)) = fx.compressor {
        compress(channels, rate, threshold, ratio);
    }
    match (fx.limiter, fx.compressor) {
        (Some(ceiling), _) => limit(channels, rate, ceiling),
        // The make-up gain can push a sudden loud start past full scale
        // while the compressor reacts; a safety limiter stops it clipping.
        (None, Some(_)) => limit(channels, rate, SAFETY_CEILING_DB),
        (None, None) => {}
    }
}

/// The ceiling of the safety limiter after the compressor when the limiter is off.
pub const SAFETY_CEILING_DB: f64 = -0.1;

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn sine(frequency: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(RATE * seconds) as usize).map(|i| amplitude * (2.0 * std::f32::consts::PI * frequency * i as f32 / RATE).sin()).collect()
    }

    /// Level (dB) of the second half (after the filters settle).
    fn level(samples: &[f32]) -> f64 {
        let tail = &samples[samples.len() / 2..];
        let rms = (tail.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / tail.len() as f64).sqrt();
        gain_to_db(rms)
    }

    fn eq_gain(frequency: f32, fx: AudioFx) -> f64 {
        let dry = sine(frequency, 0.5, 0.1);
        let mut wet = vec![dry.clone()];
        apply_audio_fx(&mut wet, RATE, &fx);
        level(&wet[0]) - level(&dry)
    }

    #[test]
    fn bands_lift_and_cut_their_own_frequencies() {
        let fx = |low_db, mid_db, high_db| AudioFx { low_db, mid_db, high_db, ..Default::default() };
        // A shelf reaches its full gain well past its corner; a bell at its centre.
        assert!((eq_gain(40.0, fx(9.0, 0.0, 0.0)) - 9.0).abs() < 1.0, "{}", eq_gain(40.0, fx(9.0, 0.0, 0.0)));
        assert!(eq_gain(4000.0, fx(9.0, 0.0, 0.0)).abs() < 0.3);
        assert!((eq_gain(1000.0, fx(0.0, -6.0, 0.0)) + 6.0).abs() < 0.2);
        assert!(eq_gain(100.0, fx(0.0, -6.0, 0.0)).abs() < 1.0);
        assert!((eq_gain(18_000.0, fx(0.0, 0.0, 6.0)) - 6.0).abs() < 1.0);
        assert!(eq_gain(500.0, fx(0.0, 0.0, 6.0)).abs() < 0.3);
    }

    #[test]
    fn the_compressor_evens_out_loud_and_quiet_parts() {
        let mut quiet = sine(440.0, 1.0, 0.05);
        quiet.extend(sine(440.0, 1.0, 0.8));
        let dry_spread = level(&quiet[quiet.len() / 2..]) - level(&quiet[..quiet.len() / 2]);
        let mut wet = vec![quiet.clone()];
        apply_audio_fx(&mut wet, RATE, &AudioFx { compressor: Some((-24.0, 4.0)), ..Default::default() });
        let half = wet[0].len() / 2;
        let wet_spread = level(&wet[0][half..]) - level(&wet[0][..half]);
        assert!(dry_spread > 20.0 && wet_spread < dry_spread - 8.0, "dry {dry_spread} dB, wet {wet_spread} dB");
        // Heavy settings with the make-up gain keep the loud part within 6 dB of where it was
        // and lift the quiet part, without going past full scale.
        let mut heavy = vec![quiet.clone()];
        apply_audio_fx(&mut heavy, RATE, &AudioFx { compressor: Some((-30.0, 8.0)), ..Default::default() });
        let loud_drop = level(&quiet[half..]) - level(&heavy[0][half..]);
        assert!(loud_drop < 6.0, "the loud part dropped {loud_drop} dB");
        assert!(level(&heavy[0][..half]) > level(&quiet[..half]) + 10.0);
        assert!(heavy[0].iter().all(|s| (s.abs() as f64) <= db_to_gain(SAFETY_CEILING_DB) + 1e-6));
    }

    #[test]
    fn the_limiter_keeps_every_sample_under_the_ceiling() {
        let mut channels = vec![sine(200.0, 0.5, 1.5), sine(300.0, 0.5, 0.5)];
        apply_audio_fx(&mut channels, RATE, &AudioFx { limiter: Some(-3.0), ..Default::default() });
        let ceiling = db_to_gain(-3.0) as f32 + 1e-6;
        assert!(channels.iter().flatten().all(|sample| sample.abs() <= ceiling));
        // Quiet audio passes untouched.
        let mut soft = vec![sine(200.0, 0.2, 0.1)];
        let before = soft.clone();
        apply_audio_fx(&mut soft, RATE, &AudioFx { limiter: Some(-1.0), ..Default::default() });
        assert_eq!(soft, before);
    }

    #[test]
    fn reads_params_and_leaves_audio_alone_by_default() {
        let fx = AudioFx::from_params(
            |key| match key {
                "audio.eq.low" => Some(30.0),
                "audio.comp.threshold" => Some(-18.0),
                _ => None,
            },
            |key| (key == "audio.comp.enabled").then_some(true),
        );
        assert_eq!(fx, AudioFx { low_db: 12.0, compressor: Some((-18.0, DEFAULT_RATIO)), ..Default::default() });
        let mut samples = vec![sine(440.0, 0.1, 0.5)];
        let before = samples.clone();
        apply_audio_fx(&mut samples, RATE, &AudioFx::default());
        assert_eq!(samples, before);
        assert!(AudioFx::default().is_identity() && !fx.is_identity());
        assert_ne!(fx.key(), AudioFx::default().key());
        assert_eq!(audio_fx_presets()[0].2, AudioFx::default());
    }
}

//! Deflicker: evens out the frame-to-frame brightness flicker of timelapses
//! and of footage under LED or fluorescent light.
//!
//! A video clip's frames are measured once (the mean Rec. 709 luma of each,
//! [`frame_level_rgba`] / [`frame_level_bgra`]) and the levels are kept on
//! the clip as a compact string ([`Levels::encode`]). When a frame is drawn,
//! its gain is the average level of its neighbours over its own level
//! ([`Levels::gain_at`]): a bright frame among darker ones is pulled down,
//! a dark one lifted. Neighbours whose level differs by more than half
//! again (a cut, a light switched on) are left out, so real changes stay.
//! The shared compositor multiplies the frame by the gain before effects.
//! Both editors use this crate (the web through WASM); checked against
//! `fixtures/deflicker/cases.json`.

/// How far the averaging reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    Off,
    Light,
    Medium,
    Strong,
}

impl Strength {
    pub const ALL: [Strength; 4] = [Strength::Off, Strength::Light, Strength::Medium, Strength::Strong];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|strength| strength.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strength::Off => "off",
            Strength::Light => "light",
            Strength::Medium => "medium",
            Strength::Strong => "strong",
        }
    }

    /// Frames averaged around each frame (itself included); 0 when off.
    pub fn window(self) -> usize {
        match self {
            Strength::Off => 0,
            Strength::Light => 5,
            Strength::Medium => 11,
            Strength::Strong => 25,
        }
    }
}

/// The weakest and strongest correction.
pub const MIN_GAIN: f64 = 0.5;
pub const MAX_GAIN: f64 = 2.0;
/// Neighbours brighter or darker than this ratio are a different shot.
pub const SAME_SHOT_RATIO: f64 = 1.5;
/// Frames darker than this (0-255 luma) are left as they are.
pub const DARK_LEVEL: f64 = 1.0;
/// Frames are measured at most this many times a second, this wide.
pub const MEASURE_FPS: f64 = 30.0;
pub const MEASURE_WIDTH: u32 = 160;

fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64
}

/// The mean luma (0-255) of an RGBA frame.
pub fn frame_level_rgba(pixels: &[u8], width: usize, height: usize) -> f64 {
    frame_level(pixels, width, height, (0, 1, 2))
}

/// The mean luma (0-255) of a BGRA frame.
pub fn frame_level_bgra(pixels: &[u8], width: usize, height: usize) -> f64 {
    frame_level(pixels, width, height, (2, 1, 0))
}

fn frame_level(pixels: &[u8], width: usize, height: usize, (r, g, b): (usize, usize, usize)) -> f64 {
    let count = (width * height).min(pixels.len() / 4);
    if count == 0 {
        return 0.0;
    }
    let sum: f64 = pixels.chunks_exact(4).take(count).map(|px| luma(px[r], px[g], px[b])).sum();
    sum / count as f64
}

/// The gain for frame `index` of `levels` with `window` frames averaged.
pub fn gain(levels: &[f64], index: usize, window: usize) -> f64 {
    let Some(&own) = levels.get(index) else { return 1.0 };
    if window < 2 || !own.is_finite() || own < DARK_LEVEL {
        return 1.0;
    }
    let reach = window / 2;
    let from = index.saturating_sub(reach);
    let to = (index + reach).min(levels.len() - 1);
    let (mut sum, mut count) = (0.0, 0usize);
    for &level in &levels[from..=to] {
        if level.is_finite() && level * SAME_SHOT_RATIO >= own && level <= own * SAME_SHOT_RATIO {
            sum += level;
            count += 1;
        }
    }
    if count == 0 {
        return 1.0;
    }
    (sum / count as f64 / own).clamp(MIN_GAIN, MAX_GAIN)
}

/// A clip's measured frames: which media, the source time of the first,
/// how many a second, and each one's level.
#[derive(Clone, Debug, PartialEq)]
pub struct Levels {
    pub media_id: String,
    pub start: f64,
    pub rate: f64,
    pub values: Vec<f64>,
}

impl Levels {
    /// `media;start;rate;v,v,...` with levels to two decimals.
    pub fn encode(&self) -> String {
        let values: Vec<String> = self.values.iter().map(|value| format!("{:.2}", value)).collect();
        format!("{};{};{};{}", self.media_id, trim_number(self.start), trim_number(self.rate), values.join(","))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(4, ';');
        let media_id = parts.next()?.to_owned();
        let start: f64 = parts.next()?.trim().parse().ok()?;
        let rate: f64 = parts.next()?.trim().parse().ok()?;
        let values = parts
            .next()?
            .split(',')
            .map(|value| value.trim().parse::<f64>().ok().filter(|value| value.is_finite()))
            .collect::<Option<Vec<f64>>>()?;
        (!media_id.is_empty() && start.is_finite() && rate.is_finite() && rate > 0.0 && !values.is_empty())
            .then_some(Self { media_id, start, rate, values })
    }

    /// The measured frame nearest a source time (clamped to the ends).
    pub fn index_at(&self, source_seconds: f64) -> usize {
        let index = ((source_seconds - self.start) * self.rate).round();
        if index.is_nan() || index < 0.0 { 0 } else { (index as usize).min(self.values.len() - 1) }
    }

    /// The gain at a source time with `window` frames averaged.
    pub fn gain_at(&self, source_seconds: f64, window: usize) -> f64 {
        gain(&self.values, self.index_at(source_seconds), window)
    }
}

/// Up to six decimals, without trailing zeros.
fn trim_number(value: f64) -> String {
    let text = format!("{:.6}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/deflicker/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn numbers(value: &Value) -> Vec<f64> {
        value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    }

    #[test]
    fn frame_levels_match_the_reference() {
        for case in cases()["levels"].as_array().unwrap() {
            let rgba: Vec<u8> = case["rgba"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let (w, h) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
            let expected = case["level"].as_f64().unwrap();
            assert!((frame_level_rgba(&rgba, w, h) - expected).abs() < 1e-9, "{case}");
            let bgra: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            assert!((frame_level_bgra(&bgra, w, h) - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn gains_match_the_reference() {
        for case in cases()["gains"].as_array().unwrap() {
            let levels = numbers(&case["levels"]);
            let window = case["window"].as_u64().unwrap() as usize;
            let expected = numbers(&case["gains"]);
            for (index, want) in expected.iter().enumerate() {
                let got = gain(&levels, index, window);
                assert!((got - want).abs() < 1e-9, "{} index {index}: {got} vs {want}", case["name"]);
            }
        }
    }

    #[test]
    fn stored_levels_parse_and_look_up_like_the_reference() {
        for case in cases()["lookups"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let parsed = Levels::parse(text);
            if case["valid"].as_bool() == Some(false) {
                assert_eq!(parsed, None, "{text}");
                continue;
            }
            let levels = parsed.expect("valid");
            assert_eq!(levels.media_id, case["mediaId"].as_str().unwrap());
            for probe in case["probes"].as_array().unwrap() {
                let got = levels.gain_at(probe["time"].as_f64().unwrap(), probe["window"].as_u64().unwrap() as usize);
                assert!((got - probe["gain"].as_f64().unwrap()).abs() < 1e-9, "{text} {probe}");
                assert_eq!(levels.index_at(probe["time"].as_f64().unwrap()) as u64, probe["index"].as_u64().unwrap());
            }
            assert_eq!(levels.encode(), case["encoded"].as_str().unwrap());
        }
    }

    #[test]
    fn strengths_round_trip() {
        for strength in Strength::ALL {
            assert_eq!(Strength::parse(strength.as_str()), Some(strength));
        }
        assert_eq!(Strength::parse("max"), None);
    }
}

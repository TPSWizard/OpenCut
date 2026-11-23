//! Colour match: one clip's colours moved onto a reference frame's, by the
//! mean and spread of each CIELAB channel (Reinhard transfer), baked into a
//! look-up table for the existing `lut` effect so both renderers draw it
//! unchanged (the web app's `src/colour-match/match.ts`, checked against the
//! same `fixtures/colour-match/cases.json`).

use crate::{Command, Effect, LutTable, Params, Project, encode_lut_param};

const WHITE: [f64; 3] = [0.95047, 1.0, 1.08883];
const D: f64 = 6.0 / 29.0;

fn decode(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f64) -> f64 {
    let v = c.clamp(0.0, 1.0);
    if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn f(t: f64) -> f64 {
    if t > D * D * D { t.cbrt() } else { t / (3.0 * D * D) + 4.0 / 29.0 }
}

fn finv(t: f64) -> f64 {
    if t > D { t * t * t } else { 3.0 * D * D * (t - 4.0 / 29.0) }
}

/// sRGB in 0..1 to CIELAB (D65).
pub fn rgb_to_lab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (lr, lg, lb) = (decode(r), decode(g), decode(b));
    let x = 0.4124564 * lr + 0.3575761 * lg + 0.1804375 * lb;
    let y = 0.2126729 * lr + 0.7151522 * lg + 0.0721750 * lb;
    let z = 0.0193339 * lr + 0.1191920 * lg + 0.9503041 * lb;
    let (fx, fy, fz) = (f(x / WHITE[0]), f(y / WHITE[1]), f(z / WHITE[2]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIELAB (D65) to sRGB in 0..1, clamped.
pub fn lab_to_rgb([l, a, b]: [f64; 3]) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let (x, y, z) = (WHITE[0] * finv(fy + a / 500.0), WHITE[1] * finv(fy), WHITE[2] * finv(fy - b / 200.0));
    [
        encode(3.2404542 * x - 1.5371385 * y - 0.4985314 * z),
        encode(-0.9692660 * x + 1.8760108 * y + 0.0415560 * z),
        encode(0.0556434 * x - 0.2040259 * y + 1.0572252 * z),
    ]
}

/// Mean and spread of each Lab channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColourStats {
    pub count: usize,
    pub mean: [f64; 3],
    pub std: [f64; 3],
}

/// Gathers Lab statistics over pixels from one or more frames.
#[derive(Clone, Debug, Default)]
pub struct ColourAccumulator {
    count: usize,
    sum: [f64; 3],
    sum_sq: [f64; 3],
}

impl ColourAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds RGBA bytes; pixels under half alpha are skipped.
    pub fn add_rgba(&mut self, data: &[u8]) {
        for pixel in data.chunks_exact(4) {
            if pixel[3] >= 128 {
                self.add_rgb(pixel[0], pixel[1], pixel[2]);
            }
        }
    }

    /// Adds BGRA bytes (decoded video frames); pixels under half alpha are skipped.
    pub fn add_bgra(&mut self, data: &[u8]) {
        for pixel in data.chunks_exact(4) {
            if pixel[3] >= 128 {
                self.add_rgb(pixel[2], pixel[1], pixel[0]);
            }
        }
    }

    fn add_rgb(&mut self, r: u8, g: u8, b: u8) {
        let lab = rgb_to_lab([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]);
        self.count += 1;
        for channel in 0..3 {
            self.sum[channel] += lab[channel];
            self.sum_sq[channel] += lab[channel] * lab[channel];
        }
    }

    /// The statistics, or `None` with no pixels.
    pub fn stats(&self) -> Option<ColourStats> {
        if self.count == 0 {
            return None;
        }
        let n = self.count as f64;
        let mean = self.sum.map(|value| value / n);
        let mut std = [0.0; 3];
        for channel in 0..3 {
            std[channel] = (self.sum_sq[channel] / n - mean[channel] * mean[channel]).max(0.0).sqrt();
        }
        Some(ColourStats { count: self.count, mean, std })
    }
}

/// How far a channel's spread may be stretched or squeezed.
pub const MIN_SPREAD_RATIO: f64 = 0.5;
pub const MAX_SPREAD_RATIO: f64 = 2.0;

/// One colour moved from `source`'s statistics onto `reference`'s.
pub fn transfer_colour(rgb: [f64; 3], source: &ColourStats, reference: &ColourStats) -> [f64; 3] {
    let lab = rgb_to_lab(rgb);
    let mut out = [0.0; 3];
    for channel in 0..3 {
        let ratio = if source.std[channel] < 1e-6 {
            1.0
        } else {
            (reference.std[channel] / source.std[channel]).clamp(MIN_SPREAD_RATIO, MAX_SPREAD_RATIO)
        };
        out[channel] = reference.mean[channel] + (lab[channel] - source.mean[channel]) * ratio;
    }
    lab_to_rgb(out)
}

/// Points a side of the generated table (the transfer is smooth).
pub const COLOUR_MATCH_LUT_SIZE: usize = 17;

/// The transfer as a look-up table, red varying fastest.
pub fn colour_match_lut(source: &ColourStats, reference: &ColourStats, size: usize) -> LutTable {
    let n = (size.max(2) - 1) as f64;
    let mut rgb = Vec::with_capacity(size * size * size * 3);
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let out = transfer_colour([r as f64 / n, g as f64 / n, b as f64 / n], source, reference);
                rgb.extend(out.map(|value| value as f32));
            }
        }
    }
    LutTable { size, rgb }
}

/// The name the generated table carries, which marks a colour-match effect.
pub const COLOUR_MATCH_NAME: &str = "Colour match";

/// The `lut` effect parameter for a match.
pub fn colour_match_param(source: &ColourStats, reference: &ColourStats) -> String {
    encode_lut_param(COLOUR_MATCH_NAME, &colour_match_lut(source, reference, COLOUR_MATCH_LUT_SIZE))
}

/// Whether a `lut` parameter came from colour match.
pub fn is_colour_match_param(param: &str) -> bool {
    param.starts_with(COLOUR_MATCH_NAME) && param[COLOUR_MATCH_NAME.len()..].starts_with('|')
}

/// The commands that give `element_id` the match: its earlier colour-match
/// effect updated in place, or a new `lut` effect at full intensity.
/// `None` when the element is missing.
pub fn colour_match_commands(project: &Project, element_id: &str, param: String) -> Option<Vec<Command>> {
    let element = project.tracks.iter().flat_map(|track| &track.elements).find(|element| element.id == element_id)?;
    let existing = element
        .effects
        .iter()
        .find(|effect| effect.kind == "lut" && is_colour_match_param(&effect.params.text("lut", "")));
    let mut params = Params::new();
    params.set("lut", param).set("intensity", 1.0);
    Some(vec![match existing {
        Some(effect) => Command::UpdateEffect {
            element_id: element_id.to_owned(),
            effect_id: effect.id.clone(),
            params: Some(params),
            enabled: Some(true),
        },
        None => {
            let mut effect = Effect { id: crate::new_id(), kind: "lut".to_owned(), params: Params::new(), enabled: true };
            effect.params.set("interpolation", "trilinear");
            effect.params.merge(&params);
            Command::AddEffect { element_id: element_id.to_owned(), effect }
        }
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triple(value: &serde_json::Value) -> [f64; 3] {
        [value[0].as_f64().unwrap(), value[1].as_f64().unwrap(), value[2].as_f64().unwrap()]
    }

    fn moments(value: &serde_json::Value) -> ColourStats {
        ColourStats { count: 1, mean: triple(&value["mean"]), std: triple(&value["std"]) }
    }

    fn assert_close(actual: [f64; 3], expected: [f64; 3], tolerance: f64, label: &str) {
        for channel in 0..3 {
            assert!((actual[channel] - expected[channel]).abs() < tolerance, "{label}: {actual:?} vs {expected:?}");
        }
    }

    #[test]
    fn matches_the_shared_cases_like_the_web() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/colour-match/cases.json")).unwrap();
        for case in fixture["lab"].as_array().unwrap() {
            let (rgb, lab) = (triple(&case["rgb"]), triple(&case["lab"]));
            assert_close(rgb_to_lab(rgb), lab, 1e-6, "lab");
            assert_close(lab_to_rgb(lab), rgb, 1e-5, "round trip");
        }
        for case in fixture["stats"].as_array().unwrap() {
            let bytes: Vec<u8> =
                case["pixels"].as_array().unwrap().iter().flat_map(|p| p.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8)).collect();
            let mut accumulator = ColourAccumulator::new();
            accumulator.add_rgba(&bytes);
            let stats = accumulator.stats().unwrap();
            assert_eq!(stats.count as u64, case["count"].as_u64().unwrap());
            assert_close(stats.mean, triple(&case["mean"]), 1e-6, "mean");
            assert_close(stats.std, triple(&case["std"]), 1e-6, "std");
            let bgra: Vec<u8> = bytes.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            let mut from_bgra = ColourAccumulator::new();
            from_bgra.add_bgra(&bgra);
            assert_close(from_bgra.stats().unwrap().mean, stats.mean, 1e-12, "bgra");
        }
        assert!(ColourAccumulator::new().stats().is_none());
        for case in fixture["transfer"].as_array().unwrap() {
            let (source, reference) = (moments(&case["source"]), moments(&case["reference"]));
            for sample in case["samples"].as_array().unwrap() {
                assert_close(transfer_colour(triple(&sample["rgb"]), &source, &reference), triple(&sample["out"]), 1e-6, "transfer");
            }
        }
    }

    #[test]
    fn writes_one_effect_and_updates_it_on_a_second_match() {
        use crate::{MediaItem, MediaKind, TrackPlacement, apply, decode_lut_param};
        let mut project = Project::new("Match");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "m".to_owned(),
                name: "m.png".to_owned(),
                kind: MediaKind::Image,
                path: "m.png".into(),
                duration: None,
                width: Some(64),
                height: Some(64),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let clip = crate::Element::from_media(project.media_item("m").unwrap(), time::MediaTime::ZERO);
        let id = clip.id.clone();
        apply(&mut project, &Command::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
        let warm = ColourStats { count: 1, mean: [60.0, 10.0, 30.0], std: [20.0, 8.0, 10.0] };
        let cool = ColourStats { count: 1, mean: [50.0, -2.0, -20.0], std: [15.0, 5.0, 12.0] };
        assert!(colour_match_commands(&project, "missing", String::new()).is_none());
        for (source, reference) in [(&warm, &cool), (&cool, &warm)] {
            let commands = colour_match_commands(&project, &id, colour_match_param(source, reference)).unwrap();
            apply(&mut project, &Command::Batch(commands)).unwrap();
        }
        let element = project.tracks.iter().flat_map(|track| &track.elements).find(|element| element.id == id).unwrap();
        assert_eq!(element.effects.len(), 1, "a second match updates the first");
        let (name, table) = decode_lut_param(&element.effects[0].params.text("lut", "")).unwrap();
        assert_eq!((name.as_str(), table.size), (COLOUR_MATCH_NAME, COLOUR_MATCH_LUT_SIZE));
        let expected = transfer_colour([0.5, 0.5, 0.5], &cool, &warm);
        let sampled = table.sample(0.5, 0.5, 0.5);
        assert_close([sampled[0] as f64, sampled[1] as f64, sampled[2] as f64], expected, 0.01, "table");
        assert!(is_colour_match_param(&element.effects[0].params.text("lut", "")));
        assert!(!is_colour_match_param("Colour matched|2|AAAA"));
    }
}

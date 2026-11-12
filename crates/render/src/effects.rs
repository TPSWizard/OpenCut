//! Turns an element's effects into compositor shader passes, with the same
//! numbers as the web app's effect definitions.

use std::collections::HashMap;

use compositor::{EffectPassDescriptor, EffectUniformValueDescriptor};
use editor::{Effect, EffectPlan, effect_spec};

const GAUSSIAN_BLUR_SHADER: &str = "gaussian-blur";
const MAX_SINGLE_PASS_SIGMA: f64 = 10.0;
const MAX_STEP: f64 = 4.0;
const MAX_EFFECTIVE_SIGMA: f64 = MAX_SINGLE_PASS_SIGMA * MAX_STEP;
const MAX_ITERATIONS: usize = 8;
/// Blur `intensity` (0–100) → sigma in pixels at the reference size.
const INTENSITY_TO_SIGMA_DIVISOR: f64 = 5.0;
const REFERENCE_WIDTH: f64 = 1920.0;
const REFERENCE_HEIGHT: f64 = 1080.0;

/// One pass group per effect, for a frame of `width` x `height` pixels at
/// `time_seconds` into the element (grain and glitch change with time).
/// Effects the renderer does not know are left out.
pub fn effect_pass_groups(
    effects: &[EffectPlan],
    width: u32,
    height: u32,
    time_seconds: f64,
) -> Vec<Vec<EffectPassDescriptor>> {
    effects
        .iter()
        .filter_map(|effect| {
            let passes = effect_passes(effect, width, height, time_seconds);
            (!passes.is_empty()).then_some(passes)
        })
        .collect()
}

/// Pixel-sized settings are given at a 1080-high reference frame and
/// scale with the frame's height, as the web app does.
fn pixel_scale(height: u32) -> f64 {
    height as f64 / REFERENCE_HEIGHT
}

fn effect_passes(effect: &EffectPlan, width: u32, height: u32, time_seconds: f64) -> Vec<EffectPassDescriptor> {
    let spec = effect_spec(&effect.kind);
    // A parameter the plan lacks falls back to the catalogue default.
    let param = |key: &str| -> f64 {
        let default = spec
            .and_then(|spec| spec.params.iter().find(|param| param.key == key))
            .map(|param| param.default)
            .unwrap_or(0.0);
        effect.params.number(key, default)
    };
    let scale = pixel_scale(height);
    match effect.kind.as_str() {
        Effect::BLUR => {
            let intensity = param("intensity").clamp(0.0, 100.0);
            let sigma_x = intensity_to_sigma(intensity, width as f64, REFERENCE_WIDTH);
            let sigma_y = intensity_to_sigma(intensity, height as f64, REFERENCE_HEIGHT);
            gaussian_blur_passes(sigma_x, sigma_y)
        }
        "color" => vec![numbers_pass(
            "color-correct",
            &[
                ("u_brightness", param("brightness")),
                ("u_contrast", param("contrast")),
                ("u_saturation", param("saturation")),
                ("u_hue", param("hue").to_radians()),
                ("u_exposure", param("exposure")),
                ("u_gamma", param("gamma")),
                ("u_temperature", param("temperature")),
                ("u_tint", param("tint")),
            ],
        )],
        "vignette" => vec![numbers_pass(
            "vignette",
            &[
                ("u_amount", param("amount")),
                ("u_radius", param("radius")),
                ("u_softness", param("softness")),
            ],
        )],
        "grain" => vec![numbers_pass(
            "film-grain",
            &[
                ("u_amount", param("amount")),
                ("u_size", (param("size") * scale).max(1.0)),
                ("u_time", time_seconds),
                ("u_seed", param("seed")),
            ],
        )],
        "rgb-split" => vec![numbers_pass(
            "rgb-split",
            &[
                ("u_amount", param("amount") * scale),
                ("u_angle", param("angle").to_radians()),
            ],
        )],
        "pixelate" => vec![numbers_pass("pixelate", &[("u_size", (param("size") * scale).max(1.0))])],
        "sharpen" => vec![numbers_pass("sharpen", &[("u_amount", param("amount"))])],
        "mirror" => {
            let mode = spec
                .and_then(|spec| spec.params.first())
                .map(|param| param.choice_index(&effect.params.text("mode", "left")))
                .unwrap_or(0);
            vec![numbers_pass("mirror", &[("u_mode", mode as f64)])]
        }
        "glitch" => vec![numbers_pass(
            "glitch",
            &[
                ("u_amount", param("amount")),
                ("u_bands", param("bands")),
                ("u_time", time_seconds),
                ("u_seed", param("seed")),
            ],
        )],
        "kaleidoscope" => vec![numbers_pass(
            "kaleidoscope",
            &[
                ("u_segments", param("segments")),
                ("u_angle", param("angle").to_radians()),
            ],
        )],
        "lift-gamma-gain" => vec![numbers_pass(
            "lift-gamma-gain",
            &[
                ("u_lift", param("lift")),
                ("u_lift_r", param("liftRed")),
                ("u_lift_g", param("liftGreen")),
                ("u_lift_b", param("liftBlue")),
                ("u_gamma", param("gamma")),
                ("u_gamma_r", param("gammaRed")),
                ("u_gamma_g", param("gammaGreen")),
                ("u_gamma_b", param("gammaBlue")),
                ("u_gain", param("gain")),
                ("u_gain_r", param("gainRed")),
                ("u_gain_g", param("gainGreen")),
                ("u_gain_b", param("gainBlue")),
            ],
        )],
        "curves" => vec![numbers_pass(
            "curves",
            &[
                ("u_master_0", param("master0")),
                ("u_master_25", param("master25")),
                ("u_master_50", param("master50")),
                ("u_master_75", param("master75")),
                ("u_master_100", param("master100")),
                ("u_red_25", param("red25")),
                ("u_red_50", param("red50")),
                ("u_red_75", param("red75")),
                ("u_green_25", param("green25")),
                ("u_green_50", param("green50")),
                ("u_green_75", param("green75")),
                ("u_blue_25", param("blue25")),
                ("u_blue_50", param("blue50")),
                ("u_blue_75", param("blue75")),
            ],
        )],
        "levels" => vec![numbers_pass(
            "levels",
            &[
                ("u_in_black", param("inputBlack")),
                ("u_in_white", param("inputWhite")),
                ("u_gamma", param("gamma")),
                ("u_out_black", param("outputBlack")),
                ("u_out_white", param("outputWhite")),
            ],
        )],
        "shadows-highlights" => vec![numbers_pass(
            "shadows-highlights",
            &[
                ("u_shadows", param("shadows")),
                ("u_highlights", param("highlights")),
                ("u_range", param("range")),
            ],
        )],
        _ => Vec::new(),
    }
}

fn numbers_pass(shader: &str, numbers: &[(&str, f64)]) -> EffectPassDescriptor {
    EffectPassDescriptor {
        shader: shader.to_owned(),
        uniforms: numbers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), EffectUniformValueDescriptor::Number(*value as f32)))
            .collect(),
    }
}

/// Blur passes for a blurred-background backdrop. The web app's presets go
/// up to 500, so unlike the effect the intensity is not capped at 100.
pub fn backdrop_blur_passes(intensity: f64, width: u32, height: u32) -> Vec<EffectPassDescriptor> {
    let intensity = intensity.max(0.0);
    gaussian_blur_passes(
        intensity_to_sigma(intensity, width as f64, REFERENCE_WIDTH),
        intensity_to_sigma(intensity, height as f64, REFERENCE_HEIGHT),
    )
}

fn intensity_to_sigma(intensity: f64, resolution: f64, reference: f64) -> f64 {
    (intensity / INTENSITY_TO_SIGMA_DIVISOR) * (resolution / reference)
}

/// Separable blur passes. Wide blurs are split into several smaller ones
/// (sigma adds in quadrature) and sample with a larger step, exactly as
/// the web app does.
pub fn gaussian_blur_passes(sigma_x: f64, sigma_y: f64) -> Vec<EffectPassDescriptor> {
    let max_sigma = sigma_x.max(sigma_y);
    if max_sigma < 0.001 {
        return Vec::new();
    }
    let iterations = ((max_sigma * max_sigma) / (MAX_EFFECTIVE_SIGMA * MAX_EFFECTIVE_SIGMA))
        .ceil()
        .max(1.0)
        .min(MAX_ITERATIONS as f64) as usize;
    let per_pass_x = sigma_x / (iterations as f64).sqrt();
    let per_pass_y = sigma_y / (iterations as f64).sqrt();
    let step_x = (per_pass_x / MAX_SINGLE_PASS_SIGMA).max(1.0);
    let step_y = (per_pass_y / MAX_SINGLE_PASS_SIGMA).max(1.0);
    let mut passes = Vec::with_capacity(iterations * 2);
    for _ in 0..iterations {
        passes.push(blur_pass(per_pass_x, step_x, [1.0, 0.0]));
        passes.push(blur_pass(per_pass_y, step_y, [0.0, 1.0]));
    }
    passes
}

fn blur_pass(sigma: f64, step: f64, direction: [f32; 2]) -> EffectPassDescriptor {
    EffectPassDescriptor {
        shader: GAUSSIAN_BLUR_SHADER.to_owned(),
        uniforms: HashMap::from([
            (
                "u_sigma".to_owned(),
                EffectUniformValueDescriptor::Number(sigma.max(0.001) as f32),
            ),
            (
                "u_step".to_owned(),
                EffectUniformValueDescriptor::Number(step as f32),
            ),
            (
                "u_direction".to_owned(),
                EffectUniformValueDescriptor::Vector(direction.to_vec()),
            ),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::Params;

    fn number(pass: &EffectPassDescriptor, name: &str) -> f32 {
        match &pass.uniforms[name] {
            EffectUniformValueDescriptor::Number(value) => *value,
            other => panic!("{name} is {other:?}"),
        }
    }

    fn plan(kind: &str, params: &[(&str, f64)]) -> EffectPlan {
        let mut values = Params::new();
        for (key, value) in params {
            values.set(*key, *value);
        }
        EffectPlan { id: "fx".to_owned(), kind: kind.to_owned(), params: values }
    }

    #[test]
    fn blur_builds_separable_passes() {
        let passes = gaussian_blur_passes(3.0, 1.5);
        assert_eq!(passes.len(), 2);
        assert_eq!(number(&passes[0], "u_sigma"), 3.0);
        assert_eq!(number(&passes[0], "u_step"), 1.0);
        assert_eq!(number(&passes[1], "u_sigma"), 1.5);
        assert!(gaussian_blur_passes(0.0, 0.0).is_empty());

        // sigma 60 needs ceil(3600/1600) = 3 iterations of 60/√3 with step > 1.
        let passes = gaussian_blur_passes(60.0, 60.0);
        assert_eq!(passes.len(), 6);
        let sigma = number(&passes[0], "u_sigma");
        assert!((sigma - 34.641).abs() < 0.01, "{sigma}");
        assert!((number(&passes[0], "u_step") - 3.4641).abs() < 0.001);
    }

    #[test]
    fn intensity_scales_with_the_frame_size() {
        let plan = plan("blur", &[("intensity", 50.0)]);
        let groups = effect_pass_groups(&[plan.clone()], 1920, 1080, 0.0);
        assert_eq!(groups.len(), 1);
        assert_eq!(number(&groups[0][0], "u_sigma"), 10.0);
        let groups = effect_pass_groups(&[plan], 960, 540, 0.0);
        assert_eq!(number(&groups[0][0], "u_sigma"), 5.0, "half-size frames blur half as much");

        let unknown = plan_unknown();
        assert!(effect_pass_groups(&[unknown], 100, 100, 0.0).is_empty());
    }

    fn plan_unknown() -> EffectPlan {
        EffectPlan { id: "x".to_owned(), kind: "sepia".to_owned(), params: Params::new() }
    }

    #[test]
    fn colour_and_stylise_effects_map_their_parameters() {
        let groups = effect_pass_groups(
            &[
                plan("color", &[("hue", 90.0), ("gamma", 1.5)]),
                plan("rgb-split", &[("amount", 10.0), ("angle", 180.0)]),
                plan("pixelate", &[("size", 20.0)]),
                plan("grain", &[("amount", 0.4)]),
            ],
            960,
            540,
            2.5,
        );
        assert_eq!(groups.len(), 4);
        let colour = &groups[0][0];
        assert_eq!(colour.shader, "color-correct");
        assert!((number(colour, "u_hue") - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(number(colour, "u_gamma"), 1.5);
        assert_eq!(number(colour, "u_brightness"), 0.0, "missing params take the catalogue default");
        let split = &groups[1][0];
        assert_eq!(number(split, "u_amount"), 5.0, "pixel offsets scale with a 540-high frame");
        assert!((number(split, "u_angle") - std::f32::consts::PI).abs() < 1e-6);
        assert_eq!(number(&groups[2][0], "u_size"), 10.0);
        let grain = &groups[3][0];
        assert_eq!(number(grain, "u_time"), 2.5);
        assert_eq!(number(grain, "u_size"), 1.0, "default grain of 2 px halves to 1 at 540");
    }

    #[test]
    fn every_catalogue_effect_fills_its_shader_exactly() {
        for spec in editor::EFFECT_CATALOGUE {
            if spec.kind == Effect::BLUR {
                continue;
            }
            let effect = Effect::from_catalogue(spec.kind).unwrap();
            let plan = EffectPlan { id: effect.id.clone(), kind: effect.kind.clone(), params: effect.params.clone() };
            let groups = effect_pass_groups(&[plan], 1920, 1080, 0.0);
            assert_eq!(groups.len(), 1, "{} builds one pass group", spec.kind);
            for pass in &groups[0] {
                let shader = effects::shader_spec(&pass.shader).unwrap_or_else(|| panic!("{} names shader {}", spec.kind, pass.shader));
                let mut names: Vec<&str> = pass.uniforms.keys().map(String::as_str).collect();
                names.sort_unstable();
                let mut declared: Vec<&str> = shader.numbers.to_vec();
                declared.sort_unstable();
                assert_eq!(names, declared, "{} sets exactly the uniforms {} declares", spec.kind, pass.shader);
            }
        }
    }

    #[test]
    fn mirror_turns_its_choice_into_a_mode_number() {
        let mut params = Params::new();
        params.set("mode", "bottom");
        let plan = EffectPlan { id: "m".to_owned(), kind: "mirror".to_owned(), params };
        let groups = effect_pass_groups(&[plan], 100, 100, 0.0);
        assert_eq!(number(&groups[0][0], "u_mode"), 3.0);
    }
}

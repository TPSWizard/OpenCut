//! Turns an element's effects into compositor shader passes, with the same
//! numbers as the web app's effect definitions.

use std::collections::HashMap;

use compositor::{EffectPassDescriptor, EffectUniformValueDescriptor};
use editor::{Effect, EffectPlan};

const GAUSSIAN_BLUR_SHADER: &str = "gaussian-blur";
const MAX_SINGLE_PASS_SIGMA: f64 = 10.0;
const MAX_STEP: f64 = 4.0;
const MAX_EFFECTIVE_SIGMA: f64 = MAX_SINGLE_PASS_SIGMA * MAX_STEP;
const MAX_ITERATIONS: usize = 8;
/// Blur `intensity` (0–100) → sigma in pixels at the reference size.
const INTENSITY_TO_SIGMA_DIVISOR: f64 = 5.0;
const REFERENCE_WIDTH: f64 = 1920.0;
const REFERENCE_HEIGHT: f64 = 1080.0;

/// One pass group per effect, for a frame of `width` x `height` pixels.
/// Effects the renderer does not know are left out.
pub fn effect_pass_groups(
    effects: &[EffectPlan],
    width: u32,
    height: u32,
) -> Vec<Vec<EffectPassDescriptor>> {
    effects
        .iter()
        .filter_map(|effect| match effect.kind.as_str() {
            Effect::BLUR => {
                let intensity = effect.params.number("intensity", 15.0).clamp(0.0, 100.0);
                let sigma_x = intensity_to_sigma(intensity, width as f64, REFERENCE_WIDTH);
                let sigma_y = intensity_to_sigma(intensity, height as f64, REFERENCE_HEIGHT);
                let passes = gaussian_blur_passes(sigma_x, sigma_y);
                (!passes.is_empty()).then_some(passes)
            }
            _ => None,
        })
        .collect()
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
        let mut params = Params::new();
        params.set("intensity", 50.0);
        let plan = EffectPlan {
            id: "fx".to_owned(),
            kind: "blur".to_owned(),
            params,
        };
        let groups = effect_pass_groups(&[plan.clone()], 1920, 1080);
        assert_eq!(groups.len(), 1);
        assert_eq!(number(&groups[0][0], "u_sigma"), 10.0);
        let groups = effect_pass_groups(&[plan], 960, 540);
        assert_eq!(number(&groups[0][0], "u_sigma"), 5.0, "half-size frames blur half as much");

        let unknown = EffectPlan {
            id: "x".to_owned(),
            kind: "sepia".to_owned(),
            params: Params::new(),
        };
        assert!(effect_pass_groups(&[unknown], 100, 100).is_empty());
    }
}

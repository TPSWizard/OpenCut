//! The HSL shader on a real (headless) GPU against `fixtures/hsl/cases.json`,
//! an independent Python reference: each case moves the hue, saturation
//! and lightness of some colour bands; greys stay exactly as they were.

use std::collections::HashMap;

use effects::{ApplyEffectsOptions, EffectPass, EffectPipeline, UniformValue};
use gpu::GpuContext;
use serde_json::Value;

#[test]
fn hsl_matches_the_reference() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/hsl/cases.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let bands: Vec<&str> = fixture["bands"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
    let pixels = fixture["pixels"].as_array().unwrap();
    let (width, height) = (pixels.len() as u32, 1);
    let bgra = context.pixels_are_bgra();
    let to_device = |px: &[u8]| if bgra { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] };
    let bytes_of = |px: &Value| -> Vec<u8> { px.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect() };
    let input: Vec<u8> = pixels.iter().flat_map(|px| to_device(&bytes_of(px))).collect();
    let source = context.upload_texture(width, height, &input, "hsl-input");
    let pipeline = EffectPipeline::new(&context);
    for case in fixture["cases"].as_array().unwrap() {
        let mut uniforms = HashMap::new();
        for (control, prefix) in [("hue", "u_hue"), ("saturation", "u_sat"), ("lightness", "u_light")] {
            for (band, value) in bands.iter().zip(case[control].as_array().unwrap()) {
                uniforms.insert(format!("{prefix}_{band}"), UniformValue::Number(value.as_f64().unwrap() as f32));
            }
        }
        let pass = EffectPass { shader: "hsl".into(), uniforms, lut: None };
        let output = pipeline
            .apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &[pass] })
            .expect("the effect runs");
        let got = context.read_texture(&output).unwrap();
        let mut worst = 0i32;
        for (index, expected) in case["output"].as_array().unwrap().iter().enumerate() {
            let expected = bytes_of(expected);
            let pixel = to_device(&got[index * 4..index * 4 + 4]);
            for channel in 0..4 {
                let difference = (pixel[channel] as i32 - expected[channel] as i32).abs();
                assert!(difference <= 2, "{} pixel {index}: {pixel:?} vs {expected:?}", case["name"]);
                worst = worst.max(difference);
            }
        }
        eprintln!("{}: worst channel difference {worst}", case["name"]);
    }
}

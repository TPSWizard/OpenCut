//! The face blur shader on a real (headless) GPU against
//! `fixtures/face-blur/cases.json`, an independent Python reference: blur
//! (two passes), pixelate and black inside face ovals; everything outside them, and a
//! frame with no faces, is left exactly as it was.

use std::collections::HashMap;

use effects::{ApplyEffectsOptions, EffectPass, EffectPipeline, UniformValue};
use gpu::GpuContext;
use serde_json::Value;

#[test]
fn face_blur_matches_the_reference_pixel_for_pixel() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/face-blur/cases.json");
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (width, height) = (cases["width"].as_u64().unwrap() as u32, cases["height"].as_u64().unwrap() as u32);
    let bgra = context.pixels_are_bgra();
    let to_device = |px: &[u8]| if bgra { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] };
    let pixels = |value: &Value| -> Vec<[u8; 4]> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|px| {
                let px: Vec<u8> = px.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
                [px[0], px[1], px[2], px[3]]
            })
            .collect()
    };
    let input_pixels = pixels(&cases["input"]);
    let input: Vec<u8> = input_pixels.iter().flat_map(|px| to_device(px)).collect();
    let source = context.upload_texture(width, height, &input, "face-blur-input");
    let pipeline = EffectPipeline::new(&context);
    for case in cases["cases"].as_array().unwrap() {
        let number = |key: &str| UniformValue::Number(case[key].as_f64().unwrap() as f32);
        let mut uniforms = HashMap::from([
            ("u_style".to_owned(), number("style")),
            ("u_strength".to_owned(), number("strength")),
            ("u_size".to_owned(), number("size")),
            ("u_feather".to_owned(), number("feather")),
        ]);
        let ovals = case["ovals"].as_array().unwrap();
        for slot in 0..3 {
            let oval: Vec<f32> =
                ovals.get(slot).map_or(vec![0.0; 4], |o| o.as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect());
            for (axis, value) in ["x", "y", "rx", "ry"].iter().zip(oval) {
                uniforms.insert(format!("u_face{slot}_{axis}"), UniformValue::Number(value));
            }
        }
        // Blur: across, then down.
        let passes: Vec<EffectPass> = if case["style"].as_u64() == Some(0) {
            [[1.0, 0.0], [0.0, 1.0]]
                .iter()
                .map(|direction| {
                    let mut uniforms = uniforms.clone();
                    uniforms.insert("u_direction".to_owned(), UniformValue::Vector(direction.to_vec()));
                    EffectPass { shader: "face-blur".into(), uniforms, lut: None }
                })
                .collect()
        } else {
            vec![EffectPass { shader: "face-blur".into(), uniforms, lut: None }]
        };
        let output = pipeline
            .apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &passes })
            .expect("the effect runs");
        let bytes = context.read_texture(&output).unwrap();
        let expected = pixels(&case["output"]);
        let mut worst = 0i32;
        for (index, (want, original)) in expected.iter().zip(&input_pixels).enumerate() {
            let got = to_device(&bytes[index * 4..index * 4 + 4]);
            for channel in 0..4 {
                worst = worst.max((got[channel] as i32 - want[channel] as i32).abs());
            }
            // Untouched pixels stay exactly as they were.
            if want == original {
                assert_eq!(got, *original, "{}: pixel {index} changed", case["name"]);
            }
        }
        eprintln!("{}: worst channel difference {worst}", case["name"]);
        assert!(worst <= 2, "{}: off by {worst}", case["name"]);
    }
}

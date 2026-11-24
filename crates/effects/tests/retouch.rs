//! The retouch shader on a real (headless) GPU against
//! `fixtures/retouch/cases.json`, an independent Python reference: blotchy
//! skin on the left is smoothed, lifted and evened; the blue on the right
//! is left exactly as it was.

use std::collections::HashMap;

use effects::{ApplyEffectsOptions, EffectPass, EffectPipeline, UniformValue};
use gpu::GpuContext;
use serde_json::Value;

#[test]
fn retouch_matches_the_reference_pixel_for_pixel() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/retouch/cases.json");
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (width, height) = (cases["width"].as_u64().unwrap() as u32, cases["height"].as_u64().unwrap() as u32);
    let bgra = context.pixels_are_bgra();
    let to_device = |px: &[u8]| if bgra { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] };
    let input: Vec<u8> = cases["input"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|px| {
            let px: Vec<u8> = px.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            to_device(&px)
        })
        .collect();
    let source = context.upload_texture(width, height, &input, "retouch-input");
    let pipeline = EffectPipeline::new(&context);
    for case in cases["cases"].as_array().unwrap() {
        let number = |key: &str| UniformValue::Number(case[key].as_f64().unwrap() as f32);
        let pass = EffectPass {
            shader: "retouch".into(),
            uniforms: HashMap::from([
                ("u_smooth".to_owned(), number("smooth")),
                ("u_brighten".to_owned(), number("brighten")),
                ("u_even".to_owned(), number("even")),
                ("u_step".to_owned(), number("step")),
            ]),
            lut: None,
        };
        let output = pipeline
            .apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &[pass] })
            .expect("the effect runs");
        let bytes = context.read_texture(&output).unwrap();
        let mut worst = 0i32;
        for (index, expected) in case["output"].as_array().unwrap().iter().enumerate() {
            let expected: Vec<u8> = expected.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let got = to_device(&bytes[index * 4..index * 4 + 4]);
            for channel in 0..4 {
                worst = worst.max((got[channel] as i32 - expected[channel] as i32).abs());
            }
        }
        eprintln!("{}: worst channel difference {worst}", case["name"]);
        assert!(worst <= 2, "{}: off by {worst}", case["name"]);
    }
}

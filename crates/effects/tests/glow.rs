//! Bloom and light leak shaders on a real (headless) GPU against
//! `fixtures/glow/cases.json`, an independent Python reference, and the
//! whole bloom chain (keep the highlights, blur, add back) on a lit square.

use std::collections::HashMap;

use effects::{ApplyEffectsOptions, EffectPass, EffectPipeline, SOURCE_TABLE, UniformValue};
use gpu::GpuContext;
use serde_json::Value;

fn bytes(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()
}

fn number(value: f64) -> UniformValue {
    UniformValue::Number(value as f32)
}

#[test]
fn bloom_and_light_leak_passes_match_the_reference() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/glow/cases.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (width, height) = (fixture["width"].as_u64().unwrap() as u32, fixture["height"].as_u64().unwrap() as u32);
    let bgra = context.pixels_are_bgra();
    let to_device = |px: &[u8]| if bgra { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] };
    let input: Vec<u8> = fixture["input"].as_array().unwrap().iter().flat_map(|px| to_device(&bytes(px))).collect();
    let source = context.upload_texture(width, height, &input, "glow-input");
    let pipeline = EffectPipeline::new(&context);
    let run = |name: String, pass: EffectPass, expected: &Value| {
        let output = pipeline
            .apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &[pass] })
            .expect("the effect runs");
        let got = context.read_texture(&output).unwrap();
        let mut worst = 0;
        for (index, want) in expected.as_array().unwrap().iter().enumerate() {
            let want = bytes(want);
            let pixel = to_device(&got[index * 4..index * 4 + 4]);
            for channel in 0..4 {
                let difference = (pixel[channel] as i32 - want[channel] as i32).abs();
                assert!(difference <= 2, "{name} pixel {index}: {pixel:?} vs {want:?}");
                worst = worst.max(difference);
            }
        }
        eprintln!("{name}: worst channel difference {worst}");
    };
    for case in fixture["extract"].as_array().unwrap() {
        let uniforms = HashMap::from([
            ("u_threshold".to_owned(), number(case["threshold"].as_f64().unwrap())),
            ("u_knee".to_owned(), number(case["knee"].as_f64().unwrap())),
        ]);
        run(format!("extract {}", case["threshold"]), EffectPass { shader: "bloom-extract".into(), uniforms, lut: None }, &case["output"]);
    }
    for case in fixture["combine"].as_array().unwrap() {
        let tint = case["tint"].as_array().unwrap();
        let uniforms = HashMap::from([
            ("u_intensity".to_owned(), number(case["intensity"].as_f64().unwrap())),
            ("u_tint_r".to_owned(), number(tint[0].as_f64().unwrap())),
            ("u_tint_g".to_owned(), number(tint[1].as_f64().unwrap())),
            ("u_tint_b".to_owned(), number(tint[2].as_f64().unwrap())),
        ]);
        let pass = EffectPass { shader: "bloom-combine".into(), uniforms, lut: Some(SOURCE_TABLE.to_owned()) };
        run(format!("combine {}", case["intensity"]), pass, &case["output"]);
    }
    for case in fixture["leak"].as_array().unwrap() {
        let mut uniforms = HashMap::new();
        for (uniform, key) in [("u_intensity", "intensity"), ("u_size", "size"), ("u_angle", "angle"), ("u_time", "time"), ("u_speed", "speed"), ("u_seed", "seed")] {
            uniforms.insert(uniform.to_owned(), number(case[key].as_f64().unwrap()));
        }
        for (colour, key) in [("colour1", "colour1"), ("colour2", "colour2")] {
            for (channel, name) in ["r", "g", "b"].iter().enumerate() {
                uniforms.insert(format!("u_{colour}_{name}"), number(case[key][channel].as_f64().unwrap()));
            }
        }
        run(format!("leak at {}", case["time"]), EffectPass { shader: "light-leak".into(), uniforms, lut: None }, &case["output"]);
    }
}

#[test]
fn bloom_glows_around_highlights_and_leaves_darker_frames_alone() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let pipeline = EffectPipeline::new(&context);
    let (width, height) = (64u32, 64u32);
    let chain = |threshold: f64| {
        let mut passes = vec![EffectPass {
            shader: "bloom-extract".into(),
            uniforms: HashMap::from([("u_threshold".to_owned(), number(threshold)), ("u_knee".to_owned(), number(0.1))]),
            lut: None,
        }];
        for direction in [[1.0, 0.0], [0.0, 1.0]] {
            passes.push(EffectPass {
                shader: "gaussian-blur".into(),
                uniforms: HashMap::from([
                    ("u_sigma".to_owned(), number(4.0)),
                    ("u_step".to_owned(), number(1.0)),
                    ("u_direction".to_owned(), UniformValue::Vector(direction.to_vec())),
                ]),
                lut: None,
            });
        }
        passes.push(EffectPass {
            shader: "bloom-combine".into(),
            uniforms: HashMap::from([
                ("u_intensity".to_owned(), number(1.0)),
                ("u_tint_r".to_owned(), number(1.0)),
                ("u_tint_g".to_owned(), number(1.0)),
                ("u_tint_b".to_owned(), number(1.0)),
            ]),
            lut: Some(SOURCE_TABLE.to_owned()),
        });
        passes
    };
    // A white 6x6 square on black: the square stays white, a glow rings it,
    // the corners stay black.
    let mut square = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            let lit = (29..35).contains(&x) && (29..35).contains(&y);
            square[index..index + 4].copy_from_slice(&if lit { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
        }
    }
    let source = context.upload_texture(width, height, &square, "bloom-square");
    let output = pipeline.apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &chain(0.8) }).unwrap();
    let got = context.read_texture(&output).unwrap();
    let at = |x: u32, y: u32| got[((y * width + x) * 4) as usize];
    eprintln!("centre {} edge+3 {} edge+8 {} corner {}", at(32, 32), at(37, 32), at(42, 32), at(2, 2));
    assert_eq!(at(32, 32), 255);
    assert!(at(37, 32) > 20, "a glow just outside the square: {}", at(37, 32));
    assert!(at(37, 32) > at(42, 32), "fading with distance");
    assert_eq!(at(2, 2), 0, "the corners stay black");
    // A frame darker than the threshold comes out exactly as it went in.
    let grey = vec![100u8; (width * height * 4) as usize].chunks(4).flat_map(|_| [100, 100, 100, 255]).collect::<Vec<u8>>();
    let source = context.upload_texture(width, height, &grey, "bloom-grey");
    let output = pipeline.apply(&context, ApplyEffectsOptions { source: &source, width, height, passes: &chain(0.8) }).unwrap();
    assert_eq!(context.read_texture(&output).unwrap(), grey);
}

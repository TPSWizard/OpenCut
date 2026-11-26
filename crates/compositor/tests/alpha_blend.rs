//! Compositing over a transparent background (row 97): full-frame solid
//! layers with straight alpha, opacity and blend modes, drawn over clear,
//! half-transparent and opaque backgrounds on a real (headless) GPU. Each
//! result is checked against `fixtures/alpha/cases.json`, made by an
//! independent Python version of the W3C source-over formula.

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor, LayerDescriptor,
    QuadTransformDescriptor,
};
use gpu::{GpuContext, wgpu};
use serde::Deserialize;

const FRAME: u32 = 16;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    background: [u8; 4],
    layers: Vec<Layer>,
    expected: [u8; 4],
}

#[derive(Deserialize)]
struct Layer {
    rgba: [u8; 4],
    opacity: f32,
    mode: String,
}

fn solid(context: &GpuContext, rgba: [u8; 4]) -> wgpu::Texture {
    const SIZE: u32 = 4;
    let pixel = if context.pixels_are_bgra() { [rgba[2], rgba[1], rgba[0], rgba[3]] } else { rgba };
    let data: Vec<u8> = (0..SIZE * SIZE).flat_map(|_| pixel).collect();
    context.upload_texture(SIZE, SIZE, &data, "solid")
}

fn mode(name: &str) -> BlendMode {
    match name {
        "normal" => BlendMode::Normal,
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "difference" => BlendMode::Difference,
        "overlay" => BlendMode::Overlay,
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn layers_over_a_transparent_background_match_the_reference() {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let mut compositor = Compositor::new(&context);
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/alpha/cases.json"))
        .expect("fixture");
    let fixture: Fixture = serde_json::from_str(&text).expect("fixture parses");
    let mut failures = Vec::new();
    for case in &fixture.cases {
        let mut items = Vec::new();
        for (index, layer) in case.layers.iter().enumerate() {
            let id = format!("{}-{index}", case.name);
            compositor.upsert_texture(id.clone(), solid(&context, layer.rgba));
            items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                texture_id: id,
                transform: QuadTransformDescriptor {
                    center_x: FRAME as f32 / 2.0,
                    center_y: FRAME as f32 / 2.0,
                    width: FRAME as f32,
                    height: FRAME as f32,
                    rotation_degrees: 0.0,
                    flip_x: false,
                    flip_y: false,
                },
                opacity: layer.opacity,
                blend_mode: mode(&layer.mode),
                effect_pass_groups: Vec::new(),
                mask: None,
                style: Default::default(),
                corner_pin: None,
                motion_blur: None,
                brightness_gain: None,
            }));
        }
        let background = case.background.map(|c| c as f32 / 255.0);
        let frame = FrameDescriptor { width: FRAME, height: FRAME, clear: CanvasClearDescriptor { color: background }, items };
        let texture = compositor.render_frame_to_texture(&context, &frame).expect("frame renders");
        let bytes = context.read_texture(&texture).expect("readback");
        let i = ((FRAME / 2 * FRAME + FRAME / 2) * 4) as usize;
        let px = &bytes[i..i + 4];
        let got = if context.pixels_are_bgra() { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] };
        let off = got.iter().zip(case.expected).map(|(a, b)| (*a as i32 - b as i32).abs()).max().unwrap_or(0);
        // Colour is only meaningful where something is there.
        let colour_off = if case.expected[3] == 0 { (got[3] as i32).abs() } else { off };
        if colour_off > 2 {
            failures.push(format!("{}: got {got:?}, expected {:?}", case.name, case.expected));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

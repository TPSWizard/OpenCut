//! Motion blur on a real (headless) GPU: a solid white quad moved during the
//! shutter over a black frame. Each pixel's grey is the share of the
//! in-between draws that cover it, which `fixtures/motion-blur/cases.json`
//! (an independent Python reference) lists.

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor, LayerDescriptor,
    MotionBlurDescriptor, QuadTransformDescriptor,
};
use gpu::{GpuContext, wgpu};
use serde_json::Value;

fn white(context: &GpuContext) -> wgpu::Texture {
    const SIZE: u32 = 4;
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue().write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &[255u8; (SIZE * SIZE * 4) as usize],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIZE * 4), rows_per_image: Some(SIZE) },
        wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
    );
    texture
}

fn layer(transform: QuadTransformDescriptor, motion_blur: Option<MotionBlurDescriptor>) -> FrameItemDescriptor {
    FrameItemDescriptor::Layer(LayerDescriptor {
        texture_id: "white".into(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effect_pass_groups: Vec::new(),
        mask: None,
        style: Default::default(),
        corner_pin: None,
        motion_blur,
    })
}

/// The green channel of every pixel, row by row (grey, so any channel).
fn render(width: u32, height: u32, item: FrameItemDescriptor) -> Option<Vec<u8>> {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    let mut compositor = Compositor::new(&context);
    compositor.upsert_texture("white".into(), white(&context));
    let frame = FrameDescriptor {
        width,
        height,
        clear: CanvasClearDescriptor { color: [0.0, 0.0, 0.0, 1.0] },
        items: vec![item],
    };
    let texture = compositor.render_frame_to_texture(&context, &frame).expect("frame renders");
    let bytes = context.read_texture(&texture).expect("readback");
    Some(bytes.chunks(4).map(|px| px[1]).collect())
}

fn quad(value: &Value) -> QuadTransformDescriptor {
    serde_json::from_value(value.clone()).expect("quad")
}

fn cases() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/motion-blur/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn lerp_quad(a: &QuadTransformDescriptor, b: &QuadTransformDescriptor) -> QuadTransformDescriptor {
    QuadTransformDescriptor {
        center_x: (a.center_x + b.center_x) / 2.0,
        center_y: (a.center_y + b.center_y) / 2.0,
        width: (a.width + b.width) / 2.0,
        height: (a.height + b.height) / 2.0,
        rotation_degrees: (a.rotation_degrees + b.rotation_degrees) / 2.0,
        flip_x: false,
        flip_y: false,
    }
}

#[test]
fn each_pixel_is_the_share_of_draws_that_cover_it() {
    for case in cases()["render"].as_array().expect("render") {
        let (width, height) = (case["width"].as_u64().unwrap() as u32, case["height"].as_u64().unwrap() as u32);
        let (from, to) = (quad(&case["from"]), quad(&case["to"]));
        let item = layer(lerp_quad(&from, &to), Some(MotionBlurDescriptor { from, to }));
        let Some(pixels) = render(width, height, item) else { return };
        for pixel in case["pixels"].as_array().unwrap() {
            let (x, y) = (pixel["x"].as_u64().unwrap() as u32, pixel["y"].as_u64().unwrap() as u32);
            let expected = pixel["alpha"].as_f64().unwrap() * 255.0;
            let got = pixels[(y * width + x) as usize] as f64;
            assert!((got - expected).abs() <= 2.0, "({x}, {y}): {got} vs {expected:.1} in {case}");
        }
    }
}

#[test]
fn without_movement_the_quad_is_drawn_once_with_hard_edges() {
    let square = QuadTransformDescriptor {
        center_x: 32.0,
        center_y: 16.0,
        width: 20.0,
        height: 10.0,
        rotation_degrees: 0.0,
        flip_x: false,
        flip_y: false,
    };
    let still = MotionBlurDescriptor { from: square.clone(), to: square.clone() };
    for item in [layer(square.clone(), None), layer(square.clone(), Some(still))] {
        let Some(pixels) = render(64, 32, item) else { return };
        let row = &pixels[16 * 64..17 * 64];
        assert_eq!(&row[20..24], &[0, 0, 255, 255]);
        assert_eq!(&row[40..44], &[255, 255, 0, 0]);
    }
}

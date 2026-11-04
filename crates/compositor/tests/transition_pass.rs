//! The transition pass on a real (headless) GPU: two solid layers blended by
//! a shader, read back and checked pixel by pixel.

use std::collections::HashMap;

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor,
    LayerDescriptor, QuadTransformDescriptor, TransitionDescriptor, TransitionParamDescriptor,
};
use gpu::{GpuContext, wgpu};

const SIZE: u32 = 8;

fn solid(context: &GpuContext, rgba: [u8; 4]) -> wgpu::Texture {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixel = if context.pixels_are_bgra() {
        [rgba[2], rgba[1], rgba[0], rgba[3]]
    } else {
        rgba
    };
    let data: Vec<u8> = (0..SIZE * SIZE).flat_map(|_| pixel).collect();
    context.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    texture
}

fn full_layer(texture_id: &str) -> LayerDescriptor {
    LayerDescriptor {
        texture_id: texture_id.to_owned(),
        transform: QuadTransformDescriptor {
            center_x: SIZE as f32 / 2.0,
            center_y: SIZE as f32 / 2.0,
            width: SIZE as f32,
            height: SIZE as f32,
            rotation_degrees: 0.0,
            flip_x: false,
            flip_y: false,
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effect_pass_groups: Vec::new(),
        mask: None,
    }
}

/// Renders `items` over black and returns straight RGBA pixels, row-major.
fn render(context: &GpuContext, compositor: &mut Compositor, items: Vec<FrameItemDescriptor>) -> Vec<[u8; 4]> {
    let frame = FrameDescriptor {
        width: SIZE,
        height: SIZE,
        clear: CanvasClearDescriptor {
            color: [0.0, 0.0, 0.0, 1.0],
        },
        items,
    };
    let texture = compositor
        .render_frame_to_texture(context, &frame)
        .expect("frame renders");
    let bytes = context.read_texture(&texture).expect("readback");
    bytes
        .chunks_exact(4)
        .map(|px| {
            if context.pixels_are_bgra() {
                [px[2], px[1], px[0], px[3]]
            } else {
                [px[0], px[1], px[2], px[3]]
            }
        })
        .collect()
}

fn setup() -> Option<(GpuContext, Compositor)> {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    let mut compositor = Compositor::new(&context);
    compositor.upsert_texture("red".into(), solid(&context, [255, 0, 0, 255]));
    compositor.upsert_texture("blue".into(), solid(&context, [0, 0, 255, 255]));
    Some((context, compositor))
}

fn close(a: u8, b: u8) -> bool {
    (a as i16 - b as i16).abs() <= 3
}

#[test]
fn a_plain_layer_still_renders() {
    let Some((context, mut compositor)) = setup() else { return };
    let pixels = render(&context, &mut compositor, vec![FrameItemDescriptor::Layer(full_layer("red"))]);
    assert!(pixels.iter().all(|px| close(px[0], 255) && close(px[2], 0)), "{:?}", pixels[0]);
}

#[test]
fn a_fade_mixes_both_sides_by_progress() {
    let Some((context, mut compositor)) = setup() else { return };
    for (progress, red, blue) in [(0.0, 255u8, 0u8), (0.25, 191, 64), (0.5, 128, 128), (1.0, 0, 255)] {
        let pixels = render(
            &context,
            &mut compositor,
            vec![FrameItemDescriptor::Transition(TransitionDescriptor {
                name: "Fade".into(),
                progress,
                params: HashMap::new(),
                from: vec![full_layer("red")],
                to: vec![full_layer("blue")],
            })],
        );
        let px = pixels[SIZE as usize * 4 + 4];
        assert!(
            close(px[0], red) && close(px[1], 0) && close(px[2], blue) && close(px[3], 255),
            "progress {progress}: got {px:?}, wanted [{red}, 0, {blue}, 255]"
        );
    }
}

#[test]
fn a_wipe_reveals_the_incoming_side_from_the_left() {
    let Some((context, mut compositor)) = setup() else { return };
    let mut params = HashMap::new();
    params.insert("direction".to_owned(), TransitionParamDescriptor::Vector(vec![1.0, 0.0]));
    params.insert("smoothness".to_owned(), TransitionParamDescriptor::Number(0.01));
    let pixels = render(
        &context,
        &mut compositor,
        vec![FrameItemDescriptor::Transition(TransitionDescriptor {
            name: "DirectionalWipe".into(),
            progress: 0.5,
            params,
            from: vec![full_layer("red")],
            to: vec![full_layer("blue")],
        })],
    );
    let row = &pixels[SIZE as usize * 4..SIZE as usize * 5];
    assert!(close(row[0][2], 255) && close(row[0][0], 0), "left edge shows `to`: {:?}", row[0]);
    assert!(close(row[7][0], 255) && close(row[7][2], 0), "right edge shows `from`: {:?}", row[7]);
}

#[test]
fn an_empty_side_is_transparent() {
    let Some((context, mut compositor)) = setup() else { return };
    let pixels = render(
        &context,
        &mut compositor,
        vec![FrameItemDescriptor::Transition(TransitionDescriptor {
            name: "Fade".into(),
            progress: 0.5,
            params: HashMap::new(),
            from: vec![full_layer("red")],
            to: Vec::new(),
        })],
    );
    // The GL Transition contract mixes straight RGBA with transparent black,
    // so colour and alpha both halve: over black that is a quarter-bright red.
    let px = pixels[SIZE as usize * 4 + 4];
    assert!(close(px[0], 64) && close(px[2], 0), "{px:?}");
}

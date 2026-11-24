//! Corner pin on a real (headless) GPU: a texture with four coloured
//! quarters pinned to a trapezoid on a 128-pixel frame, read back pixel by
//! pixel at the places the reference maths puts each quarter.

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, CornerPin, FrameDescriptor, FrameItemDescriptor, LayerDescriptor,
    QuadTransformDescriptor, apply_pin_matrix, square_to_pin,
};
use gpu::{GpuContext, wgpu};

const FRAME: u32 = 128;
const BOX: f32 = 96.0;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// 16×16: red top left, green top right, blue bottom right, white bottom left.
fn quarters(context: &GpuContext) -> wgpu::Texture {
    const SIZE: u32 = 16;
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("quarters"),
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bgra = context.pixels_are_bgra();
    let mut data = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let rgba = match (x < SIZE / 2, y < SIZE / 2) {
                (true, true) => RED,
                (false, true) => GREEN,
                (false, false) => BLUE,
                (true, false) => WHITE,
            };
            data.extend(if bgra { [rgba[2], rgba[1], rgba[0], rgba[3]] } else { rgba });
        }
    }
    context.queue().write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIZE * 4), rows_per_image: Some(SIZE) },
        wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
    );
    texture
}

fn layer(pin: Option<CornerPin>, rotation_degrees: f32) -> FrameItemDescriptor {
    FrameItemDescriptor::Layer(LayerDescriptor {
        texture_id: "quarters".into(),
        transform: QuadTransformDescriptor {
            center_x: FRAME as f32 / 2.0,
            center_y: FRAME as f32 / 2.0,
            width: BOX,
            height: BOX,
            rotation_degrees,
            flip_x: false,
            flip_y: false,
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effect_pass_groups: Vec::new(),
        mask: None,
        style: Default::default(),
        corner_pin: pin,
        motion_blur: None,
    })
}

struct Pixels(Vec<u8>, bool);

impl Pixels {
    fn at(&self, x: f64, y: f64) -> [u8; 4] {
        let (x, y) = (x.floor() as u32, y.floor() as u32);
        let i = ((y * FRAME + x) * 4) as usize;
        let px = &self.0[i..i + 4];
        if self.1 { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] }
    }
}

fn render(item: FrameItemDescriptor) -> Option<Pixels> {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    let mut compositor = Compositor::new(&context);
    compositor.upsert_texture("quarters".into(), quarters(&context));
    let frame = FrameDescriptor {
        width: FRAME,
        height: FRAME,
        clear: CanvasClearDescriptor { color: [0.0, 0.0, 0.0, 1.0] },
        items: vec![item],
    };
    let texture = compositor.render_frame_to_texture(&context, &frame).expect("frame renders");
    let bytes = context.read_texture(&texture).expect("readback");
    Some(Pixels(bytes, context.pixels_are_bgra()))
}

/// Box units to frame pixels, for an unrotated box centred on the frame.
fn frame_point([x, y]: [f64; 2]) -> (f64, f64) {
    let origin = (FRAME as f64 - BOX as f64) / 2.0;
    (origin + x * BOX as f64, origin + y * BOX as f64)
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| (*a as i32 - b as i32).abs() <= 8)
}

const TRAPEZOID: CornerPin = [[0.3, 0.1], [0.7, 0.1], [1.0, 0.9], [0.0, 0.9]];

#[test]
fn each_quarter_lands_where_the_perspective_puts_it() {
    let Some(pixels) = render(layer(Some(TRAPEZOID), 0.0)) else { return };
    let corners = TRAPEZOID.map(|[x, y]| [x as f64, y as f64]);
    let forward = square_to_pin(&corners).unwrap();
    for (uv, colour) in [([0.25, 0.25], RED), ([0.75, 0.25], GREEN), ([0.75, 0.75], BLUE), ([0.25, 0.75], WHITE)] {
        let (x, y) = frame_point(apply_pin_matrix(&forward, uv).unwrap());
        assert!(close(pixels.at(x, y), colour), "uv {uv:?} at ({x:.1}, {y:.1}): {:?}", pixels.at(x, y));
    }
    // Outside the trapezoid but inside the box: the background shows.
    for point in [[0.05, 0.15], [0.95, 0.15], [0.5, 0.05], [0.5, 0.95]] {
        let (x, y) = frame_point(point);
        assert!(close(pixels.at(x, y), [0, 0, 0, 255]), "{point:?} should be empty: {:?}", pixels.at(x, y));
    }
    // Perspective, not a stretch: the middle row of the picture sits above
    // the middle of the trapezoid's height (the far edge is narrower).
    let (_, mid_y) = frame_point(apply_pin_matrix(&forward, [0.5, 0.5]).unwrap());
    assert!(mid_y < frame_point([0.5, 0.5]).1 - 1.0, "middle at {mid_y}");
}

#[test]
fn without_a_pin_the_box_is_filled() {
    let Some(pixels) = render(layer(None, 0.0)) else { return };
    for (point, colour) in [([0.05, 0.05], RED), ([0.95, 0.05], GREEN), ([0.95, 0.95], BLUE), ([0.05, 0.95], WHITE)] {
        let (x, y) = frame_point(point);
        assert!(close(pixels.at(x, y), colour), "{point:?}: {:?}", pixels.at(x, y));
    }
}

#[test]
fn the_pin_turns_with_the_clip() {
    let Some(pixels) = render(layer(Some(TRAPEZOID), 90.0)) else { return };
    // A quarter turn clockwise (y down): box point (x, y) goes to the frame
    // centre plus (-(y - 0.5), x - 0.5) box widths.
    let forward = square_to_pin(&TRAPEZOID.map(|[x, y]| [x as f64, y as f64])).unwrap();
    for (uv, colour) in [([0.25, 0.25], RED), ([0.75, 0.75], BLUE)] {
        let [bx, by] = apply_pin_matrix(&forward, uv).unwrap();
        let centre = FRAME as f64 / 2.0;
        let (x, y) = (centre - (by - 0.5) * BOX as f64, centre + (bx - 0.5) * BOX as f64);
        assert!(close(pixels.at(x, y), colour), "uv {uv:?} at ({x:.1}, {y:.1}): {:?}", pixels.at(x, y));
    }
}

#[test]
fn a_shape_that_cannot_be_drawn_falls_back_to_the_box() {
    let bow_tie: CornerPin = [[1.0, 0.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let Some(pixels) = render(layer(Some(bow_tie), 0.0)) else { return };
    let (x, y) = frame_point([0.05, 0.05]);
    assert!(close(pixels.at(x, y), RED), "{:?}", pixels.at(x, y));
}

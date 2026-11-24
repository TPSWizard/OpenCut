//! Rounded corners, outline and drop shadow on a real (headless) GPU: a
//! small red square on a 64-pixel frame, read back pixel by pixel.

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor, LayerDescriptor,
    LayerOutlineDescriptor, LayerShadowDescriptor, LayerStyleDescriptor, QuadTransformDescriptor,
};
use gpu::{GpuContext, wgpu};

const FRAME: u32 = 64;
const SQUARE: f32 = 16.0;

fn solid(context: &GpuContext, rgba: [u8; 4]) -> wgpu::Texture {
    const SIZE: u32 = 4;
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixel = if context.pixels_are_bgra() { [rgba[2], rgba[1], rgba[0], rgba[3]] } else { rgba };
    let data: Vec<u8> = (0..SIZE * SIZE).flat_map(|_| pixel).collect();
    context.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIZE * 4), rows_per_image: Some(SIZE) },
        wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
    );
    texture
}

fn square(style: LayerStyleDescriptor) -> FrameItemDescriptor {
    FrameItemDescriptor::Layer(LayerDescriptor {
        texture_id: "red".into(),
        transform: QuadTransformDescriptor {
            center_x: FRAME as f32 / 2.0,
            center_y: FRAME as f32 / 2.0,
            width: SQUARE,
            height: SQUARE,
            rotation_degrees: 0.0,
            flip_x: false,
            flip_y: false,
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effect_pass_groups: Vec::new(),
        mask: None,
        style,
        corner_pin: None,
    })
}

/// Renders the square over `background` and returns a pixel reader.
fn render(
    context: &GpuContext,
    compositor: &mut Compositor,
    background: [f32; 4],
    style: LayerStyleDescriptor,
) -> Pixels {
    let frame = FrameDescriptor {
        width: FRAME,
        height: FRAME,
        clear: CanvasClearDescriptor { color: background },
        items: vec![square(style)],
    };
    let texture = compositor.render_frame_to_texture(context, &frame).expect("frame renders");
    let bytes = context.read_texture(&texture).expect("readback");
    Pixels { bytes, bgra: context.pixels_are_bgra() }
}

struct Pixels {
    bytes: Vec<u8>,
    bgra: bool,
}

impl Pixels {
    /// Straight RGBA at (x, y).
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * FRAME + x) * 4) as usize;
        let px = &self.bytes[i..i + 4];
        if self.bgra { [px[2], px[1], px[0], px[3]] } else { [px[0], px[1], px[2], px[3]] }
    }
}

fn setup() -> Option<(GpuContext, Compositor)> {
    let Ok(context) = pollster::block_on(GpuContext::new()) else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    let mut compositor = Compositor::new(&context);
    compositor.upsert_texture("red".into(), solid(&context, [255, 0, 0, 255]));
    Some((context, compositor))
}

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
// The square covers pixels 24..40 on both axes.

#[test]
fn a_plain_square_has_sharp_square_corners() {
    let Some((context, mut compositor)) = setup() else { return };
    let pixels = render(&context, &mut compositor, BLACK, LayerStyleDescriptor::default());
    let px = |x, y| pixels.at(x, y);
    assert_eq!(px(24, 24)[0], 255, "corner pixel is red");
    assert_eq!(px(22, 32)[0], 0, "outside is black");
}

#[test]
fn rounded_corners_cut_the_corners_and_keep_the_sides() {
    let Some((context, mut compositor)) = setup() else { return };
    let style = LayerStyleDescriptor { corner_radius: 8.0, ..Default::default() };
    let pixels = render(&context, &mut compositor, BLACK, style);
    let px = |x, y| pixels.at(x, y);
    assert!(px(24, 24)[0] < 20, "the corner is cut: {:?}", px(24, 24));
    assert!(px(24, 32)[0] > 240, "the middle of a side stays: {:?}", px(24, 32));
    assert!(px(32, 32)[0] > 250);
}

#[test]
fn an_outline_rings_the_square() {
    let Some((context, mut compositor)) = setup() else { return };
    let style = LayerStyleDescriptor {
        outline: Some(LayerOutlineDescriptor { width: 4.0, color: [1.0, 1.0, 1.0, 1.0] }),
        ..Default::default()
    };
    let pixels = render(&context, &mut compositor, BLACK, style);
    let px = |x, y| pixels.at(x, y);
    let ring = px(22, 32);
    assert!(ring[0] > 240 && ring[1] > 240 && ring[2] > 240, "2 px outside is white: {ring:?}");
    assert_eq!(px(16, 32), [0, 0, 0, 255], "8 px outside is the background");
    let inside = px(32, 32);
    assert!(inside[0] > 250 && inside[1] < 5, "the square stays red on top: {inside:?}");
}

#[test]
fn a_drop_shadow_sits_below_right_and_a_glow_all_round() {
    let Some((context, mut compositor)) = setup() else { return };
    let style = LayerStyleDescriptor {
        shadow: Some(LayerShadowDescriptor { color: [0.0, 0.0, 0.0, 1.0], offset_x: 10.0, offset_y: 10.0, blur: 0.0 }),
        ..Default::default()
    };
    let pixels = render(&context, &mut compositor, WHITE, style);
    let px = |x, y| pixels.at(x, y);
    assert!(px(45, 45)[0] < 10, "the shadow is below right: {:?}", px(45, 45));
    assert!(px(20, 20)[0] > 245, "nothing above left: {:?}", px(20, 20));
    assert!(px(30, 30)[0] > 250 && px(30, 30)[1] < 5, "the square is on top");

    let glow = LayerStyleDescriptor {
        shadow: Some(LayerShadowDescriptor { color: [0.0, 1.0, 0.0, 1.0], offset_x: 0.0, offset_y: 0.0, blur: 6.0 }),
        ..Default::default()
    };
    let pixels = render(&context, &mut compositor, BLACK, glow);
    let px = |x, y| pixels.at(x, y);
    let (left, right) = (px(21, 32), px(42, 32));
    assert!(left[1] > 30 && right[1] > 30, "green glow on both sides: {left:?} {right:?}");
    assert!((left[1] as i16 - right[1] as i16).abs() < 12, "evenly: {left:?} {right:?}");
    assert!(px(4, 32)[1] < 3, "fading out further away: {:?}", px(4, 32));
}

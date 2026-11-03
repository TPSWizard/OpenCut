//! Mask shapes rasterised on the CPU into frame-sized coverage textures.
//!
//! The web app draws a mask with Canvas 2D in the element's own pixel
//! space and then transforms it onto the frame; here the frame is walked
//! once and each pixel is mapped back through the element's quad into the
//! shape's space, with a one-pixel anti-aliased edge. The compositor
//! feathers and applies the result.

use compositor::QuadTransformDescriptor;
use editor::{Mask, Params};

/// The web app's floor for a mask's width and height (fraction of the element).
const MIN_MASK_DIMENSION: f64 = 0.01;

/// Coverage of one mask shape over a `width` x `height` frame as straight
/// RGBA with every channel equal to the coverage (the compositor reads the
/// alpha). Unknown shapes cover everything, so an unsupported mask shows
/// the element unchanged rather than hiding it.
pub fn rasterize_mask(
    kind: &str,
    params: &Params,
    quad: &QuadTransformDescriptor,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let shape = match kind {
        Mask::RECTANGLE => Shape::Rectangle,
        Mask::ELLIPSE => Shape::Ellipse,
        _ => return vec![255; (width * height * 4) as usize],
    };
    let geometry = Geometry::new(params, quad);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let coverage = geometry.coverage(shape, x as f64 + 0.5, y as f64 + 0.5);
            let value = (coverage * 255.0).round() as u8;
            let index = ((y * width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&[value, value, value, value]);
        }
    }
    pixels
}

#[derive(Clone, Copy)]
enum Shape {
    Rectangle,
    Ellipse,
}

/// Everything needed to map a frame pixel into the shape's local space,
/// where the shape is axis-aligned and centred on the origin.
struct Geometry {
    quad_center: (f64, f64),
    /// Undoes the quad's rotation.
    quad_rotation: (f64, f64),
    flip: (f64, f64),
    /// The shape's centre in element pixels from the element's centre.
    shape_center: (f64, f64),
    /// Undoes the shape's own rotation.
    shape_rotation: (f64, f64),
    /// Half the shape's size in element pixels.
    half: (f64, f64),
}

impl Geometry {
    fn new(params: &Params, quad: &QuadTransformDescriptor) -> Self {
        let element_width = quad.width.max(1.0) as f64;
        let element_height = quad.height.max(1.0) as f64;
        let quad_angle = -(quad.rotation_degrees as f64).to_radians();
        let shape_angle = -params.number("rotation", 0.0).to_radians();
        let mask_width = params.number("width", 0.6).max(MIN_MASK_DIMENSION) * element_width;
        let mask_height = params.number("height", 0.6).max(MIN_MASK_DIMENSION) * element_height;
        Self {
            quad_center: (quad.center_x as f64, quad.center_y as f64),
            quad_rotation: (quad_angle.cos(), quad_angle.sin()),
            flip: (
                if quad.flip_x { -1.0 } else { 1.0 },
                if quad.flip_y { -1.0 } else { 1.0 },
            ),
            shape_center: (
                params.number("centerX", 0.0) * element_width,
                params.number("centerY", 0.0) * element_height,
            ),
            shape_rotation: (shape_angle.cos(), shape_angle.sin()),
            half: (mask_width / 2.0, mask_height / 2.0),
        }
    }

    /// 0 outside, 1 inside, in between across a one-pixel edge.
    fn coverage(&self, shape: Shape, x: f64, y: f64) -> f64 {
        // Frame → element pixels (centred, unrotated, unflipped).
        let dx = x - self.quad_center.0;
        let dy = y - self.quad_center.1;
        let (cos, sin) = self.quad_rotation;
        let ex = (dx * cos - dy * sin) * self.flip.0;
        let ey = (dx * sin + dy * cos) * self.flip.1;
        // Element → shape space.
        let sx = ex - self.shape_center.0;
        let sy = ey - self.shape_center.1;
        let (cos, sin) = self.shape_rotation;
        let lx = sx * cos - sy * sin;
        let ly = sx * sin + sy * cos;
        let (half_width, half_height) = self.half;
        let distance = match shape {
            Shape::Rectangle => (lx.abs() - half_width).max(ly.abs() - half_height),
            Shape::Ellipse => {
                let radius = ((lx / half_width).powi(2) + (ly / half_height).powi(2)).sqrt();
                (radius - 1.0) * half_width.min(half_height)
            }
        };
        (0.5 - distance).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(center_x: f32, center_y: f32, width: f32, height: f32, rotation: f32) -> QuadTransformDescriptor {
        QuadTransformDescriptor {
            center_x,
            center_y,
            width,
            height,
            rotation_degrees: rotation,
            flip_x: false,
            flip_y: false,
        }
    }

    fn alpha(pixels: &[u8], width: u32, x: u32, y: u32) -> u8 {
        pixels[((y * width + x) * 4 + 3) as usize]
    }

    #[test]
    fn rectangle_covers_the_middle_of_the_element() {
        let mask = Mask::rectangle();
        // A 100x100 element centred in a 200x100 frame; the mask is 60x60.
        let pixels = rasterize_mask(&mask.kind, &mask.params, &quad(100.0, 50.0, 100.0, 100.0, 0.0), 200, 100);
        assert_eq!(alpha(&pixels, 200, 100, 50), 255, "centre");
        assert_eq!(alpha(&pixels, 200, 125, 50), 255, "inside the 60% box");
        assert_eq!(alpha(&pixels, 200, 140, 50), 0, "outside the box, inside the element");
        assert_eq!(alpha(&pixels, 200, 10, 50), 0, "outside the element");

        // An edge through the middle of a pixel covers it half.
        let mut mask = Mask::rectangle();
        mask.params.set("width", 0.61);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &quad(100.0, 50.0, 100.0, 100.0, 0.0), 200, 100);
        let edge = alpha(&pixels, 200, 130, 50);
        assert!((120..=135).contains(&edge), "anti-aliased edge: {edge}");
    }

    #[test]
    fn ellipse_and_offset_and_rotation() {
        let mut mask = Mask::ellipse();
        mask.params.set("width", 1.0).set("height", 1.0);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &quad(50.0, 50.0, 100.0, 100.0, 0.0), 100, 100);
        assert_eq!(alpha(&pixels, 100, 50, 50), 255);
        assert_eq!(alpha(&pixels, 100, 50, 2), 255, "top of the circle");
        assert_eq!(alpha(&pixels, 100, 3, 3), 0, "corner is outside the circle");

        // Shifted right by a quarter of the element.
        let mut shifted = Mask::rectangle();
        shifted.params.set("centerX", 0.25).set("width", 0.2).set("height", 1.0);
        let pixels = rasterize_mask(&shifted.kind, &shifted.params, &quad(50.0, 50.0, 100.0, 100.0, 0.0), 100, 100);
        assert_eq!(alpha(&pixels, 100, 75, 50), 255);
        assert_eq!(alpha(&pixels, 100, 50, 50), 0);

        // A thin vertical bar rotated 90° by the mask becomes horizontal.
        let mut bar = Mask::rectangle();
        bar.params.set("width", 0.1).set("height", 1.0).set("rotation", 90.0);
        let pixels = rasterize_mask(&bar.kind, &bar.params, &quad(50.0, 50.0, 100.0, 100.0, 0.0), 100, 100);
        assert_eq!(alpha(&pixels, 100, 90, 50), 255, "along the rotated bar");
        assert_eq!(alpha(&pixels, 100, 50, 90), 0, "where the unrotated bar was");

        // The same rotation applied by the element's quad gives the same picture.
        let mut bar = Mask::rectangle();
        bar.params.set("width", 0.1).set("height", 1.0);
        let pixels = rasterize_mask(&bar.kind, &bar.params, &quad(50.0, 50.0, 100.0, 100.0, 90.0), 100, 100);
        assert_eq!(alpha(&pixels, 100, 90, 50), 255);
        assert_eq!(alpha(&pixels, 100, 50, 90), 0);
    }

    #[test]
    fn unknown_shapes_cover_everything() {
        let pixels = rasterize_mask("heart", &Params::new(), &quad(5.0, 5.0, 10.0, 10.0, 0.0), 10, 10);
        assert!(pixels.iter().all(|&value| value == 255));
    }
}

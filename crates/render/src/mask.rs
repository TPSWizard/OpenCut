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
    let geometry = Geometry::new(kind, params, quad);
    let shape = match kind {
        Mask::RECTANGLE | Mask::CINEMATIC_BARS => Shape::Rectangle,
        Mask::ELLIPSE => Shape::Ellipse,
        Mask::SPLIT => Shape::Split,
        Mask::DIAMOND => Shape::Diamond,
        Mask::STAR => Shape::Polygon(Polygon::new(star_vertices(geometry.half))),
        Mask::HEART => Shape::Polygon(Polygon::new(heart_vertices(geometry.half))),
        _ => return vec![255; (width * height * 4) as usize],
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let coverage = geometry.coverage(&shape, x as f64 + 0.5, y as f64 + 0.5);
            let value = (coverage * 255.0).round() as u8;
            let index = ((y * width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&[value, value, value, value]);
        }
    }
    pixels
}

enum Shape {
    Rectangle,
    Ellipse,
    /// Everything on the positive side of the local x axis.
    Split,
    Diamond,
    Polygon(Polygon),
}

/// A closed polygon in shape space with its bounding box.
struct Polygon {
    vertices: Vec<(f64, f64)>,
    min: (f64, f64),
    max: (f64, f64),
}

impl Polygon {
    fn new(vertices: Vec<(f64, f64)>) -> Self {
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &vertices {
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
        }
        Self { vertices, min, max }
    }

    /// Signed distance to the outline: negative inside.
    fn distance(&self, x: f64, y: f64) -> f64 {
        if x < self.min.0 - 1.0 || x > self.max.0 + 1.0 || y < self.min.1 - 1.0 || y > self.max.1 + 1.0 {
            return 1.0;
        }
        let mut inside = false;
        let mut nearest = f64::INFINITY;
        let count = self.vertices.len();
        for index in 0..count {
            let (ax, ay) = self.vertices[index];
            let (bx, by) = self.vertices[(index + 1) % count];
            // Distance to the segment.
            let (ex, ey) = (bx - ax, by - ay);
            let length_squared = ex * ex + ey * ey;
            let t = if length_squared > 0.0 {
                (((x - ax) * ex + (y - ay) * ey) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (px, py) = (ax + ex * t - x, ay + ey * t - y);
            nearest = nearest.min(px * px + py * py);
            // Even-odd crossing test.
            if (ay > y) != (by > y) && x < ax + (y - ay) * ex / ey {
                inside = !inside;
            }
        }
        if inside { -nearest.sqrt() } else { nearest.sqrt() }
    }
}

/// The web app's ten-pointed star: outer vertices at the half size, inner
/// ones at 45% of it, starting at the top.
fn star_vertices((half_width, half_height): (f64, f64)) -> Vec<(f64, f64)> {
    (0..10)
        .map(|index| {
            let ratio = if index % 2 == 0 { 1.0 } else { 0.45 };
            let angle = index as f64 * std::f64::consts::PI / 5.0 - std::f64::consts::FRAC_PI_2;
            (half_width * ratio * angle.cos(), half_height * ratio * angle.sin())
        })
        .collect()
}

/// The web app's heart: two cubic Bézier curves from the notch down to the
/// point and back, flattened into straight segments.
fn heart_vertices((half_width, half_height): (f64, f64)) -> Vec<(f64, f64)> {
    const SEGMENTS: usize = 32;
    let (w, h) = (half_width, half_height);
    let notch = (0.0, -h * 0.475);
    let point = (0.0, h * 0.725);
    let curves = [
        [notch, (w, -h * 1.225), (w, -h * 0.125), point],
        [point, (-w, -h * 0.125), (-w, -h * 1.225), notch],
    ];
    let mut vertices = Vec::with_capacity(SEGMENTS * 2);
    for [p0, p1, p2, p3] in curves {
        for step in 0..SEGMENTS {
            let t = step as f64 / SEGMENTS as f64;
            let u = 1.0 - t;
            let weight = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            vertices.push((
                weight[0] * p0.0 + weight[1] * p1.0 + weight[2] * p2.0 + weight[3] * p3.0,
                weight[0] * p0.1 + weight[1] * p1.1 + weight[2] * p2.1 + weight[3] * p3.1,
            ));
        }
    }
    vertices
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
    fn new(kind: &str, params: &Params, quad: &QuadTransformDescriptor) -> Self {
        let element_width = quad.width.max(1.0) as f64;
        let element_height = quad.height.max(1.0) as f64;
        let quad_angle = -(quad.rotation_degrees as f64).to_radians();
        let shape_angle = -params.number("rotation", 0.0).to_radians();
        let mut mask_width = params.number("width", 0.6).max(MIN_MASK_DIMENSION) * element_width;
        if kind == Mask::CINEMATIC_BARS {
            // The band always spans at least the element's width.
            mask_width = mask_width.max(element_width);
        }
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
    fn coverage(&self, shape: &Shape, x: f64, y: f64) -> f64 {
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
            // The web app keeps the side the rotation's normal points to.
            Shape::Split => -lx,
            // Distance to the rhombus's edge lines |x|/w + |y|/h = 1.
            Shape::Diamond => {
                (lx.abs() / half_width + ly.abs() / half_height - 1.0)
                    / (1.0 / (half_width * half_width) + 1.0 / (half_height * half_height)).sqrt()
            }
            Shape::Polygon(polygon) => polygon.distance(lx, ly),
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
        let pixels = rasterize_mask("freeform", &Params::new(), &quad(5.0, 5.0, 10.0, 10.0, 0.0), 10, 10);
        assert!(pixels.iter().all(|&value| value == 255));
    }

    #[test]
    fn split_diamond_star_heart_and_bars() {
        let frame = quad(50.0, 50.0, 100.0, 100.0, 0.0);
        let raster = |mask: &Mask| rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);

        // Split keeps the side the rotation points to: right at 0°, bottom at 90°.
        let mut split = Mask::shape(Mask::SPLIT);
        let pixels = raster(&split);
        assert_eq!(alpha(&pixels, 100, 75, 50), 255, "right of the line");
        assert_eq!(alpha(&pixels, 100, 25, 50), 0, "left of the line");
        split.params.set("rotation", 90.0).set("centerY", 0.2);
        let pixels = raster(&split);
        assert_eq!(alpha(&pixels, 100, 50, 90), 255, "below the shifted line");
        assert_eq!(alpha(&pixels, 100, 50, 60), 0, "above it");

        // Diamond: 60% box, so tips at 20 and 80.
        let diamond = Mask::shape(Mask::DIAMOND);
        let pixels = raster(&diamond);
        assert_eq!(alpha(&pixels, 100, 50, 50), 255, "centre");
        assert_eq!(alpha(&pixels, 100, 50, 22), 255, "just inside the top tip");
        assert_eq!(alpha(&pixels, 100, 70, 30), 0, "the box corner is cut off");

        // Star: the top point reaches the box edge, the valley beside it does not.
        let star = Mask::shape(Mask::STAR);
        let pixels = raster(&star);
        assert_eq!(alpha(&pixels, 100, 50, 50), 255, "centre");
        assert_eq!(alpha(&pixels, 100, 50, 23), 255, "top point");
        assert_eq!(alpha(&pixels, 100, 62, 28), 0, "between the top and upper-right points");

        // Heart: lobes either side of the notch, nothing above the notch.
        let heart = Mask::shape(Mask::HEART);
        let pixels = raster(&heart);
        assert_eq!(alpha(&pixels, 100, 50, 50), 255, "centre");
        assert_eq!(alpha(&pixels, 100, 35, 35), 255, "left lobe");
        assert_eq!(alpha(&pixels, 100, 65, 35), 255, "right lobe");
        assert_eq!(alpha(&pixels, 100, 50, 30), 0, "the notch");
        assert_eq!(alpha(&pixels, 100, 50, 75), 0, "below the point");

        // Cinematic bars: a band spanning the whole width, whatever `width` says.
        let mut bars = Mask::shape(Mask::CINEMATIC_BARS);
        bars.params.set("width", 0.1);
        let pixels = raster(&bars);
        assert_eq!(alpha(&pixels, 100, 2, 50), 255, "left edge of the band");
        assert_eq!(alpha(&pixels, 100, 97, 50), 255, "right edge of the band");
        assert_eq!(alpha(&pixels, 100, 50, 10), 0, "above the band");
    }
}

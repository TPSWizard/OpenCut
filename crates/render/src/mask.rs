//! Mask shapes rasterised on the CPU into frame-sized coverage textures.
//!
//! The web app draws a mask with Canvas 2D in the element's own pixel
//! space and then transforms it onto the frame; here the frame is walked
//! once and each pixel is mapped back through the element's quad into the
//! shape's space, with a one-pixel anti-aliased edge. The compositor
//! feathers and applies the result.

use compositor::QuadTransformDescriptor;
use editor::{Mask, Params, parse_hex_color};

use crate::text::TextRaster;

/// The web app's floor for a mask's width and height (fraction of the element).
const MIN_MASK_DIMENSION: f64 = 0.01;
/// Straight pieces per Bézier segment of a freeform path.
const FREEFORM_SEGMENTS: usize = 16;

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
    let Some(shape) = Shape::new(kind, params, &geometry) else {
        return vec![255; (width * height * 4) as usize];
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

/// Coverage of a text mask: `glyphs` (the mask's text rasterised white
/// against the element's height) centred on the mask's centre, scaled by
/// `scale` and rotated, sampled bilinearly. Blank text is an inactive mask
/// and covers everything, as in the web app.
pub fn rasterize_text_mask(
    params: &Params,
    quad: &QuadTransformDescriptor,
    width: u32,
    height: u32,
    glyphs: &TextRaster,
) -> Vec<u8> {
    if params.text("content", "").trim().is_empty() {
        return vec![255; (width * height * 4) as usize];
    }
    let coverage = text_coverage(params, quad, width, height, glyphs);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for (index, value) in coverage.iter().enumerate() {
        let value = (value * 255.0).round() as u8;
        if value > 0 {
            pixels[index * 4..index * 4 + 4].copy_from_slice(&[value, value, value, value]);
        }
    }
    pixels
}

/// The outline of a text mask's glyphs as a stroke layer (see
/// [`rasterize_mask_stroke`]): the band is measured from the glyph
/// coverage with a distance transform, which gives the round joins the web
/// app strokes text with.
pub fn rasterize_text_mask_stroke(
    params: &Params,
    quad: &QuadTransformDescriptor,
    width: u32,
    height: u32,
    glyphs: &TextRaster,
    scale: f32,
) -> Option<Vec<u8>> {
    let stroke_width = params.number("strokeWidth", 0.0) * scale.max(0.0) as f64;
    if stroke_width <= 0.0 || params.text("content", "").trim().is_empty() {
        return None;
    }
    let color = parse_hex_color(&params.text("strokeColor", "#ffffff"))?;
    let offset = match params.text("strokeAlign", "center").as_ref() {
        "inside" => -stroke_width / 2.0,
        "outside" => stroke_width / 2.0,
        _ => 0.0,
    };
    let coverage = text_coverage(params, quad, width, height, glyphs);
    let distance = signed_distance(&coverage, width as usize, height as usize);
    let rgb = [
        (color[2] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[0] * 255.0).round() as u8,
    ];
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for (index, distance) in distance.iter().enumerate() {
        let band = (0.5 - ((distance - offset).abs() - stroke_width / 2.0)).clamp(0.0, 1.0);
        if band <= 0.0 {
            continue;
        }
        let alpha = (band * color[3] as f64 * 255.0).round() as u8;
        pixels[index * 4..index * 4 + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
    }
    Some(pixels)
}

/// Glyph coverage (0–1) of every frame pixel for a text mask.
fn text_coverage(
    params: &Params,
    quad: &QuadTransformDescriptor,
    width: u32,
    height: u32,
    glyphs: &TextRaster,
) -> Vec<f64> {
    let geometry = Geometry::new(Mask::TEXT, params, quad);
    let scale = params.number("scale", 1.0).max(0.01);
    let (glyph_width, glyph_height) = (glyphs.width as f64, glyphs.height as f64);
    let alpha = |x: i64, y: i64| -> f64 {
        if x < 0 || y < 0 || x >= glyphs.width as i64 || y >= glyphs.height as i64 {
            return 0.0;
        }
        glyphs.bgra[((y as u32 * glyphs.width + x as u32) * 4 + 3) as usize] as f64 / 255.0
    };
    let mut coverage = vec![0.0f64; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let (lx, ly) = geometry.local(x as f64 + 0.5, y as f64 + 0.5);
            // Glyph raster coordinates, pixel centres at .5.
            let gx = lx / scale + glyph_width / 2.0 - 0.5;
            let gy = ly / scale + glyph_height / 2.0 - 0.5;
            if gx < -1.0 || gy < -1.0 || gx > glyph_width || gy > glyph_height {
                continue;
            }
            let (x0, y0) = (gx.floor(), gy.floor());
            let (fx, fy) = (gx - x0, gy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let value = alpha(x0, y0) * (1.0 - fx) * (1.0 - fy)
                + alpha(x0 + 1, y0) * fx * (1.0 - fy)
                + alpha(x0, y0 + 1) * (1.0 - fx) * fy
                + alpha(x0 + 1, y0 + 1) * fx * fy;
            coverage[(y * width + x) as usize] = value.clamp(0.0, 1.0);
        }
    }
    coverage
}

/// Signed distance (negative inside) of every pixel to the edge of the
/// covered area, from two exact Euclidean distance transforms
/// (Felzenszwalb & Huttenlocher) with half-pixel anti-aliasing at the edge.
fn signed_distance(coverage: &[f64], width: usize, height: usize) -> Vec<f64> {
    const FAR: f64 = 1e12;
    let inside: Vec<f64> = coverage.iter().map(|&c| if c >= 0.5 { 0.0 } else { FAR }).collect();
    let outside: Vec<f64> = coverage.iter().map(|&c| if c < 0.5 { 0.0 } else { FAR }).collect();
    let to_covered = euclidean_distance_squared(inside, width, height);
    let to_clear = euclidean_distance_squared(outside, width, height);
    coverage
        .iter()
        .enumerate()
        .map(|(index, &c)| {
            // The edge runs halfway between a covered and a clear pixel
            // centre; a partly covered pixel moves it by its coverage.
            if c >= 0.5 {
                1.5 - c - to_clear[index].sqrt()
            } else {
                to_covered[index].sqrt() - 0.5 - c
            }
        })
        .collect()
}

/// Squared distance of every cell to the nearest cell whose value is zero.
fn euclidean_distance_squared(mut grid: Vec<f64>, width: usize, height: usize) -> Vec<f64> {
    let mut row = vec![0.0; width.max(height)];
    let mut out = vec![0.0; width.max(height)];
    let mut v = vec![0usize; width.max(height)];
    let mut z = vec![0.0f64; width.max(height) + 1];
    for y in 0..height {
        row[..width].copy_from_slice(&grid[y * width..(y + 1) * width]);
        distance_transform_1d(&row[..width], &mut out[..width], &mut v, &mut z);
        grid[y * width..(y + 1) * width].copy_from_slice(&out[..width]);
    }
    for x in 0..width {
        for y in 0..height {
            row[y] = grid[y * width + x];
        }
        distance_transform_1d(&row[..height], &mut out[..height], &mut v, &mut z);
        for y in 0..height {
            grid[y * width + x] = out[y];
        }
    }
    grid
}

fn distance_transform_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        out[q] = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// The mask's outline as a straight-alpha BGRA layer the size of the frame:
/// a band `strokeWidth` canvas pixels wide (times `scale` for this frame)
/// in `strokeColor`, centred on the edge or kept inside or outside it
/// (`strokeAlign`), as the web app draws it over the masked element. `None`
/// when there is no stroke or the shape is unknown. Feather and inversion
/// do not touch the stroke.
pub fn rasterize_mask_stroke(
    kind: &str,
    params: &Params,
    quad: &QuadTransformDescriptor,
    width: u32,
    height: u32,
    scale: f32,
) -> Option<Vec<u8>> {
    let stroke_width = params.number("strokeWidth", 0.0) * scale.max(0.0) as f64;
    if stroke_width <= 0.0 {
        return None;
    }
    let color = parse_hex_color(&params.text("strokeColor", "#ffffff"))?;
    let mut geometry = Geometry::new(kind, params, quad);
    // The band needs exact distances out to its outer edge.
    geometry.margin = stroke_width / 2.0 + 1.0;
    let shape = Shape::new(kind, params, &geometry)?;
    // Negative distances are inside, so an inside stroke sits at -w/2.
    let offset = match params.text("strokeAlign", "center").as_ref() {
        "inside" => -stroke_width / 2.0,
        "outside" => stroke_width / 2.0,
        _ => 0.0,
    };
    let rgb = [
        (color[2] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[0] * 255.0).round() as u8,
    ];
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let distance = geometry.distance(&shape, x as f64 + 0.5, y as f64 + 0.5);
            let coverage = (0.5 - ((distance - offset).abs() - stroke_width / 2.0)).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let alpha = (coverage * color[3] as f64 * 255.0).round() as u8;
            let index = ((y * width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
        }
    }
    Some(pixels)
}

enum Shape {
    Rectangle,
    Ellipse,
    /// Everything on the positive side of the local x axis.
    Split,
    Diamond,
    Polygon(Polygon),
}

impl Shape {
    /// `None` for a kind that is not rendered natively.
    fn new(kind: &str, params: &Params, geometry: &Geometry) -> Option<Self> {
        Some(match kind {
            Mask::RECTANGLE | Mask::CINEMATIC_BARS => Self::Rectangle,
            Mask::ELLIPSE => Self::Ellipse,
            Mask::SPLIT => Self::Split,
            Mask::DIAMOND => Self::Diamond,
            Mask::STAR => Self::Polygon(Polygon::new(star_vertices(geometry.half))),
            Mask::HEART => Self::Polygon(Polygon::new(heart_vertices(geometry.half))),
            // An open path is not active yet (the web app's `isActive`), so
            // the element shows unmasked while it is being drawn.
            Mask::FREEFORM => Self::Polygon(Polygon::new(freeform_vertices(params, geometry.element_size)?)),
            _ => return None,
        })
    }
}

/// A freeform path flattened into a polygon in shape space, or `None`
/// when it is open or has fewer than three points (not an active mask).
/// Points are in units of the element's size, scaled by `scale`; each
/// segment is a cubic Bézier through the points' handles.
fn freeform_vertices(params: &Params, (element_width, element_height): (f64, f64)) -> Option<Vec<(f64, f64)>> {
    if !params.bool("closed", false) {
        return None;
    }
    let scale = params.number("scale", 1.0);
    let path = params.get("path")?.as_json();
    let number = |point: &serde_json::Value, key: &str| point.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let points: Vec<[(f64, f64); 3]> = path
        .as_array()?
        .iter()
        .filter(|point| point.get("x").is_some() && point.get("y").is_some())
        .map(|point| {
            let to_pixels = |x: f64, y: f64| (x * element_width * scale, y * element_height * scale);
            let (x, y) = (number(point, "x"), number(point, "y"));
            [
                to_pixels(x, y),
                to_pixels(x + number(point, "inX"), y + number(point, "inY")),
                to_pixels(x + number(point, "outX"), y + number(point, "outY")),
            ]
        })
        .collect();
    if points.len() < 3 {
        return None;
    }
    let mut vertices = Vec::with_capacity(points.len() * FREEFORM_SEGMENTS);
    for index in 0..points.len() {
        let [p0, _, p1] = points[index];
        let [p3, p2, _] = points[(index + 1) % points.len()];
        for step in 0..FREEFORM_SEGMENTS {
            let t = step as f64 / FREEFORM_SEGMENTS as f64;
            let u = 1.0 - t;
            let weight = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            vertices.push((
                weight[0] * p0.0 + weight[1] * p1.0 + weight[2] * p2.0 + weight[3] * p3.0,
                weight[0] * p0.1 + weight[1] * p1.1 + weight[2] * p2.1 + weight[3] * p3.1,
            ));
        }
    }
    Some(vertices)
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

    /// Signed distance to the outline: negative inside. Farther than
    /// `margin` from the bounding box the distance to the box is returned
    /// instead (a lower bound), which keeps a frame-sized raster cheap.
    fn distance(&self, x: f64, y: f64, margin: f64) -> f64 {
        let dx = (self.min.0 - x).max(x - self.max.0).max(0.0);
        let dy = (self.min.1 - y).max(y - self.max.1).max(0.0);
        let outside = (dx * dx + dy * dy).sqrt();
        if outside > margin {
            return outside;
        }
        let mut winding = 0i32;
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
            // Nonzero winding, the Canvas 2D fill rule the web app uses.
            let cross = ex * (y - ay) - (x - ax) * ey;
            if ay <= y {
                if by > y && cross > 0.0 {
                    winding += 1;
                }
            } else if by <= y && cross < 0.0 {
                winding -= 1;
            }
        }
        if winding != 0 { -nearest.sqrt() } else { nearest.sqrt() }
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
    /// The element's size in frame pixels.
    element_size: (f64, f64),
    /// How far from a polygon's bounding box distances must stay exact.
    margin: f64,
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
            element_size: (element_width, element_height),
            margin: 1.0,
        }
    }

    /// 0 outside, 1 inside, in between across a one-pixel edge.
    fn coverage(&self, shape: &Shape, x: f64, y: f64) -> f64 {
        (0.5 - self.distance(shape, x, y)).clamp(0.0, 1.0)
    }

    /// A frame pixel in the shape's local space: element pixels from the
    /// shape's centre, with the element's and the shape's rotation undone.
    fn local(&self, x: f64, y: f64) -> (f64, f64) {
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
        (sx * cos - sy * sin, sx * sin + sy * cos)
    }

    /// Signed distance from a frame pixel to the shape's edge, negative
    /// inside, in frame pixels (approximate for ellipses).
    fn distance(&self, shape: &Shape, x: f64, y: f64) -> f64 {
        let (lx, ly) = self.local(x, y);
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
            Shape::Polygon(polygon) => polygon.distance(lx, ly, self.margin),
        };
        distance
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
        let pixels = rasterize_mask("text", &Params::new(), &quad(5.0, 5.0, 10.0, 10.0, 0.0), 10, 10);
        assert!(pixels.iter().all(|&value| value == 255));
        assert!(rasterize_mask_stroke("text", &Params::new(), &quad(5.0, 5.0, 10.0, 10.0, 0.0), 10, 10, 1.0).is_none());
    }

    #[test]
    fn freeform_paths_fill_when_closed() {
        let frame = quad(50.0, 50.0, 100.0, 100.0, 0.0);
        // A square from -30% to +30% of the element, no handles.
        let square = serde_json::json!([
            {"id": "a", "x": -0.3, "y": -0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
            {"id": "b", "x": 0.3, "y": -0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
            {"id": "c", "x": 0.3, "y": 0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
            {"id": "d", "x": -0.3, "y": 0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
        ]);
        let mut mask = Mask::shape(Mask::FREEFORM);
        mask.params.set("path", square.clone()).set("closed", true);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);
        assert_eq!(alpha(&pixels, 100, 50, 50), 255, "centre");
        assert_eq!(alpha(&pixels, 100, 25, 25), 255, "inside the square");
        assert_eq!(alpha(&pixels, 100, 15, 50), 0, "outside the square");

        // Open: not an active mask yet, so the element shows whole.
        mask.params.set("closed", false);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);
        assert!(pixels.iter().all(|&value| value == 255));
        assert!(rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 1.0).is_none());

        // The path may also arrive as JSON text; scale doubles it, so the
        // edge moves from 20 to 10; centre and rotation apply as usual.
        mask.params.set("path", square.to_string()).set("closed", true).set("scale", 2.0);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);
        assert_eq!(alpha(&pixels, 100, 12, 50), 255, "scaled edge");
        mask.params.set("scale", 1.0).set("centerX", 0.3);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);
        assert_eq!(alpha(&pixels, 100, 95, 50), 255, "shifted right");
        assert_eq!(alpha(&pixels, 100, 40, 50), 0);

        // Bulging handles: the right edge bows out past the straight line.
        let bowed = serde_json::json!([
            {"id": "a", "x": -0.3, "y": -0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
            {"id": "b", "x": 0.3, "y": -0.3, "inX": 0, "inY": 0, "outX": 0.3, "outY": 0},
            {"id": "c", "x": 0.3, "y": 0.3, "inX": 0.3, "inY": 0, "outX": 0, "outY": 0},
            {"id": "d", "x": -0.3, "y": 0.3, "inX": 0, "inY": 0, "outX": 0, "outY": 0},
        ]);
        let mut mask = Mask::shape(Mask::FREEFORM);
        mask.params.set("path", bowed).set("closed", true);
        let pixels = rasterize_mask(&mask.kind, &mask.params, &frame, 100, 100);
        assert_eq!(alpha(&pixels, 100, 90, 50), 255, "the bow reaches past x=80");
        assert_eq!(alpha(&pixels, 100, 96, 22), 0, "but not near the corners");
    }

    #[test]
    fn text_masks_sample_the_glyph_raster() {
        let frame = quad(50.0, 50.0, 100.0, 100.0, 0.0);
        // A fake 10x10 "glyph" raster: opaque in its left half only.
        let mut bgra = vec![0u8; 10 * 10 * 4];
        for y in 0..10 {
            for x in 0..5 {
                bgra[(y * 10 + x) * 4 + 3] = 255;
            }
        }
        let glyphs = TextRaster { width: 10, height: 10, bgra };
        let mut mask = Mask::shape(Mask::TEXT);
        let pixels = rasterize_text_mask(&mask.params, &frame, 100, 100, &glyphs);
        assert_eq!(alpha(&pixels, 100, 47, 50), 255, "left half of the glyph box, centred on the element");
        assert_eq!(alpha(&pixels, 100, 53, 50), 0, "right half is clear");
        assert_eq!(alpha(&pixels, 100, 47, 60), 0, "below the glyph box");

        // Scaled 4x and shifted a quarter to the right: box is 40 wide at x 55..95.
        mask.params.set("scale", 4.0).set("centerX", 0.25);
        let pixels = rasterize_text_mask(&mask.params, &frame, 100, 100, &glyphs);
        assert_eq!(alpha(&pixels, 100, 60, 50), 255);
        assert_eq!(alpha(&pixels, 100, 80, 50), 0);
        assert_eq!(alpha(&pixels, 100, 60, 40), 255, "20 px tall above the centre");
        assert_eq!(alpha(&pixels, 100, 60, 25), 0);

        // Rotated 90°: the opaque half turns to the top.
        mask.params.set("centerX", 0.0).set("rotation", 90.0);
        let pixels = rasterize_text_mask(&mask.params, &frame, 100, 100, &glyphs);
        assert_eq!(alpha(&pixels, 100, 50, 40), 255, "above the centre");
        assert_eq!(alpha(&pixels, 100, 50, 60), 0, "below");

        // Blank text is no mask at all.
        mask.params.set("content", "  ");
        let pixels = rasterize_text_mask(&mask.params, &frame, 100, 100, &glyphs);
        assert!(pixels.iter().all(|&value| value == 255));
    }

    #[test]
    fn text_mask_strokes_follow_the_glyph_edges() {
        let frame = quad(50.0, 50.0, 100.0, 100.0, 0.0);
        // The 10x10 "glyph" raster again: opaque in its left half (x 45..50 on the frame).
        let mut bgra = vec![0u8; 10 * 10 * 4];
        for y in 0..10 {
            for x in 0..5 {
                bgra[(y * 10 + x) * 4 + 3] = 255;
            }
        }
        let glyphs = TextRaster { width: 10, height: 10, bgra };
        let mut mask = Mask::shape(Mask::TEXT);
        assert!(rasterize_text_mask_stroke(&mask.params, &frame, 100, 100, &glyphs, 1.0).is_none());
        mask.params.set("strokeWidth", 2.0).set("strokeColor", "#00ff00");
        let pixels = rasterize_text_mask_stroke(&mask.params, &frame, 100, 100, &glyphs, 1.0).unwrap();
        let at = |x: u32, y: u32| pixels[((y * 100 + x) * 4 + 3) as usize];
        assert!(at(50, 50) > 0 && at(49, 50) > 0, "the band straddles the right edge at x=50");
        assert_eq!(at(47, 50), 0, "inside, off the band");
        assert_eq!(at(53, 50), 0, "outside, off the band");
        assert!(at(47, 45) > 0, "the top edge is stroked too");
        assert_eq!(at(5, 5), 0, "far away");
        let index = ((50 * 100 + 50) * 4) as usize;
        assert_eq!(&pixels[index..index + 3], &[0, 255, 0], "green in BGRA");

        // Outside alignment: the band moves out of the glyph.
        mask.params.set("strokeAlign", "outside");
        let pixels = rasterize_text_mask_stroke(&mask.params, &frame, 100, 100, &glyphs, 1.0).unwrap();
        assert_eq!(pixels[((50 * 100 + 48) * 4 + 3) as usize], 0);
        assert!(pixels[((50 * 100 + 51) * 4 + 3) as usize] > 0);
    }

    #[test]
    fn distance_transform_measures_to_the_nearest_covered_pixel() {
        // A 5x5 grid with one covered pixel in the middle.
        let mut coverage = vec![0.0; 25];
        coverage[12] = 1.0;
        let distance = signed_distance(&coverage, 5, 5);
        assert!((distance[12] - -0.5).abs() < 1e-9, "centre: half a pixel inside: {}", distance[12]);
        assert!((distance[13] - 0.5).abs() < 1e-9, "next door: half a pixel outside: {}", distance[13]);
        assert!((distance[0] - (8f64.sqrt() - 0.5)).abs() < 1e-9, "corner: {}", distance[0]);
        assert!((distance[2] - 1.5).abs() < 1e-9, "{}", distance[2]);
    }

    #[test]
    fn strokes_follow_the_outline() {
        let frame = quad(50.0, 50.0, 100.0, 100.0, 0.0);
        let mut mask = Mask::rectangle();
        assert!(rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 1.0).is_none(), "no width, no stroke");
        mask.params.set("strokeWidth", 4.0).set("strokeColor", "#ff0000");
        // The 60% box's right edge is at x=80: a centred 4 px band covers 78..82.
        let pixels = rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 1.0).unwrap();
        let at = |x: u32| {
            let index = ((50 * 100 + x) * 4) as usize;
            [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
        };
        assert_eq!(at(79), [0, 0, 255, 255], "red in BGRA, inside the band");
        assert_eq!(at(81), [0, 0, 255, 255]);
        assert_eq!(at(75)[3], 0, "inside the box, off the band");
        assert_eq!(at(85)[3], 0, "outside");
        assert_eq!(at(50)[3], 0, "the middle is not stroked");

        mask.params.set("strokeAlign", "inside");
        let pixels = rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 1.0).unwrap();
        assert_eq!(pixels[((50 * 100 + 77) * 4 + 3) as usize], 255, "inside: 76..80");
        assert_eq!(pixels[((50 * 100 + 81) * 4 + 3) as usize], 0);

        mask.params.set("strokeAlign", "outside");
        let pixels = rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 1.0).unwrap();
        assert_eq!(pixels[((50 * 100 + 82) * 4 + 3) as usize], 255, "outside: 80..84");
        assert_eq!(pixels[((50 * 100 + 78) * 4 + 3) as usize], 0);

        // The width is in canvas pixels: at half scale the band halves.
        mask.params.set("strokeAlign", "center");
        let pixels = rasterize_mask_stroke(&mask.kind, &mask.params, &frame, 100, 100, 0.5).unwrap();
        assert_eq!(pixels[((50 * 100 + 79) * 4 + 3) as usize], 255);
        assert_eq!(pixels[((50 * 100 + 77) * 4 + 3) as usize], 0);

        // A polygon's stroke stays on its outline: nothing far from a wide-stroked heart.
        let mut heart = Mask::shape(Mask::HEART);
        heart.params.set("strokeWidth", 14.0);
        let pixels = rasterize_mask_stroke(&heart.kind, &heart.params, &frame, 100, 100, 1.0).unwrap();
        assert_eq!(pixels[((5 * 100 + 5) * 4 + 3) as usize], 0, "far corner");
        assert_eq!(pixels[((50 * 100 + 50) * 4 + 3) as usize], 0, "centre");
        assert!(pixels[((30 * 100 + 50) * 4 + 3) as usize] > 0, "on the notch");
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

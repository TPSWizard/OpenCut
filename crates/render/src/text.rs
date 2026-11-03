//! Text elements become textures: cosmic-text shapes and rasterises them
//! with the system's fonts.

use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
    Wrap,
};
use editor::{TextAlign, TextStyle};

/// A rasterised text block: straight-alpha BGRA, tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRaster {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub struct TextRasterizer {
    font_system: FontSystem,
    cache: SwashCache,
}

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRasterizer {
    /// Loads the system font database; takes a moment, so keep one around.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            cache: SwashCache::new(),
        }
    }

    /// Lays `style.content` out within `max_width` pixels (wrapping at
    /// words) and rasterises it, including any background box.
    pub fn rasterize(&mut self, style: &TextStyle, max_width: f32) -> TextRaster {
        let font_size = style.font_size.max(1.0);
        let line_height = (font_size * style.line_height).max(1.0);
        let metrics = Metrics::new(font_size, line_height);
        let (padding_x, padding_y) = style
            .background
            .as_ref()
            .map(|background| (background.padding_x.max(0.0), background.padding_y.max(0.0)))
            .unwrap_or((0.0, 0.0));
        let wrap_width = (max_width - 2.0 * padding_x).max(font_size);

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_wrap(&mut self.font_system, Wrap::WordOrGlyph);
        buffer.set_size(&mut self.font_system, Some(wrap_width), None);
        let family = style.font_family.trim();
        let attrs = Attrs::new()
            .family(if family.is_empty() {
                Family::SansSerif
            } else {
                Family::Name(family)
            })
            .weight(if style.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            })
            .style(if style.italic {
                Style::Italic
            } else {
                Style::Normal
            })
            .letter_spacing(style.letter_spacing);
        let content = if style.content.is_empty() {
            " "
        } else {
            style.content.as_str()
        };
        buffer.set_text(&mut self.font_system, content, &attrs, Shaping::Advanced);
        let align = match style.align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        };
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Shrink the box to the widest line so alignment is relative to the
        // text itself, not to the whole canvas.
        let (text_width, text_height) = measure(&buffer);
        let text_width = text_width.ceil().max(1.0);
        buffer.set_size(&mut self.font_system, Some(text_width), None);
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(&mut self.font_system, false);
        let (_, text_height) = {
            let (_, measured) = measure(&buffer);
            (text_width, measured.max(text_height))
        };

        let width = (text_width + 2.0 * padding_x).ceil().max(1.0) as u32;
        let height = (text_height + 2.0 * padding_y).ceil().max(1.0) as u32;
        let mut bgra = vec![0u8; (width * height * 4) as usize];

        if let Some(background) = &style.background {
            fill_rounded_rect(
                &mut bgra,
                width,
                height,
                background.color,
                background.corner_radius.max(0.0),
            );
        }

        let color = Color::rgba(
            to_byte(style.color[0]),
            to_byte(style.color[1]),
            to_byte(style.color[2]),
            to_byte(style.color[3]),
        );
        let offset_x = padding_x.round() as i32;
        let offset_y = padding_y.round() as i32;
        buffer.draw(
            &mut self.font_system,
            &mut self.cache,
            color,
            |x, y, w, h, color| {
                if color.a() == 0 {
                    return;
                }
                for py in y..y + h as i32 {
                    for px in x..x + w as i32 {
                        let (px, py) = (px + offset_x, py + offset_y);
                        if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                            continue;
                        }
                        let index = ((py as u32 * width + px as u32) * 4) as usize;
                        blend_over(&mut bgra[index..index + 4], color);
                    }
                }
            },
        );

        TextRaster {
            width,
            height,
            bgra,
        }
    }
}

fn measure(buffer: &Buffer) -> (f32, f32) {
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    (width, height)
}

fn to_byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Straight-alpha "source over" of a glyph pixel onto the raster.
fn blend_over(dst: &mut [u8], src: Color) {
    let sa = src.a() as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    let src_rgb = [src.b() as f32, src.g() as f32, src.r() as f32];
    for channel in 0..3 {
        let d = dst[channel] as f32;
        let s = src_rgb[channel];
        let out = (s * sa + d * da * (1.0 - sa)) / out_a;
        dst[channel] = out.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn fill_rounded_rect(bgra: &mut [u8], width: u32, height: u32, color: [f32; 4], radius: f32) {
    let radius = radius.min(width as f32 / 2.0).min(height as f32 / 2.0);
    let pixel = [
        to_byte(color[2]),
        to_byte(color[1]),
        to_byte(color[0]),
        to_byte(color[3]),
    ];
    for y in 0..height {
        for x in 0..width {
            let coverage = rounded_rect_coverage(x as f32 + 0.5, y as f32 + 0.5, width as f32, height as f32, radius);
            if coverage <= 0.0 {
                continue;
            }
            let index = ((y * width + x) * 4) as usize;
            bgra[index] = pixel[0];
            bgra[index + 1] = pixel[1];
            bgra[index + 2] = pixel[2];
            bgra[index + 3] = (pixel[3] as f32 * coverage).round() as u8;
        }
    }
}

/// 1 inside the rounded rectangle, fading over one pixel at the corners.
fn rounded_rect_coverage(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 1.0;
    }
    let cx = if x < radius {
        radius
    } else if x > width - radius {
        width - radius
    } else {
        return 1.0;
    };
    let cy = if y < radius {
        radius
    } else if y > height - radius {
        height - radius
    } else {
        return 1.0;
    };
    let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{TextBackground, TextStyle};

    fn style(content: &str) -> TextStyle {
        TextStyle {
            content: content.to_owned(),
            font_family: "Arial".to_owned(),
            font_size: 40.0,
            color: [1.0, 0.0, 0.0, 1.0],
            align: TextAlign::Center,
            bold: false,
            italic: false,
            line_height: 1.2,
            letter_spacing: 0.0,
            background: None,
        }
    }

    #[test]
    fn rasterises_text_with_the_requested_color() {
        let mut rasterizer = TextRasterizer::new();
        let raster = rasterizer.rasterize(&style("Hello"), 1000.0);
        assert!(raster.width > 40 && raster.width < 400, "width {}", raster.width);
        assert!(raster.height >= 40 && raster.height <= 60, "height {}", raster.height);
        let opaque: Vec<&[u8]> = raster
            .bgra
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 200)
            .collect();
        assert!(!opaque.is_empty(), "no glyph pixels were drawn");
        assert!(opaque.iter().all(|pixel| pixel[2] > 200 && pixel[1] < 40 && pixel[0] < 40), "stored as BGRA red");
    }

    #[test]
    fn wraps_and_adds_a_background() {
        let mut rasterizer = TextRasterizer::new();
        let one_line = rasterizer.rasterize(&style("one two three four"), 2000.0);
        let wrapped = rasterizer.rasterize(&style("one two three four"), 150.0);
        assert!(wrapped.height > one_line.height);
        assert!(wrapped.width <= 150);

        let mut boxed = style("Hi");
        boxed.background = Some(TextBackground {
            color: [0.0, 0.0, 1.0, 1.0],
            corner_radius: 8.0,
            padding_x: 10.0,
            padding_y: 6.0,
        });
        let raster = rasterizer.rasterize(&boxed, 1000.0);
        let plain = rasterizer.rasterize(&style("Hi"), 1000.0);
        assert_eq!(raster.width, plain.width + 20);
        assert_eq!(raster.height, plain.height + 12);
        // Corner pixel is outside the rounded corner; the edge midpoint is blue.
        assert_eq!(raster.bgra[3], 0);
        let mid = ((raster.height / 2) * raster.width * 4) as usize;
        assert_eq!(&raster.bgra[mid..mid + 4], &[255, 0, 0, 255]);
    }
}

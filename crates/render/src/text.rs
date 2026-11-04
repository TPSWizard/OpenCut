//! Text elements become textures: cosmic-text shapes and rasterises them
//! with the system's fonts.

use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
    Wrap,
};
use editor::{HighlightMode, TextAlign, TextDecoration, TextHighlight, TextStyle};

/// Box padding and shape around a highlighted word, in ems (as the web app).
const HIGHLIGHT_BOX_PADDING_X: f32 = 0.18;
const HIGHLIGHT_BOX_PADDING_Y: f32 = 0.08;
const HIGHLIGHT_BOX_HEIGHT: f32 = 1.1;
const HIGHLIGHT_BOX_RADIUS: f32 = 0.2;

/// A whitespace-separated token of the content: which line it is on and
/// its byte range within that line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WordSpan {
    line: usize,
    start: usize,
    end: usize,
}

/// The content's words in order, as the web app numbers caption words.
fn word_spans(content: &str) -> Vec<WordSpan> {
    let mut spans = Vec::new();
    for (line, text) in content.split('\n').enumerate() {
        let mut start = None;
        for (index, ch) in text.char_indices() {
            match (ch.is_whitespace(), start) {
                (false, None) => start = Some(index),
                (true, Some(begin)) => {
                    spans.push(WordSpan { line, start: begin, end: index });
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(begin) = start {
            spans.push(WordSpan { line, start: begin, end: text.len() });
        }
    }
    spans
}

fn to_color(rgba: [f32; 4]) -> Color {
    Color::rgba(to_byte(rgba[0]), to_byte(rgba[1]), to_byte(rgba[2]), to_byte(rgba[3]))
}

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

    /// Lays `style.content` out (lines break only at newlines, as in the
    /// web app) and rasterises it, including any background box. The
    /// raster stays centred on the text, so a box offset does not move it.
    pub fn rasterize(&mut self, style: &TextStyle) -> TextRaster {
        let font_size = style.font_size.max(1.0);
        let line_height = (font_size * style.line_height).max(1.0);
        let metrics = Metrics::new(font_size, line_height);

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_wrap(&mut self.font_system, Wrap::None);
        buffer.set_size(&mut self.font_system, None, None);
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
            // cosmic-text takes letter spacing in ems; the parameter is pixels.
            .letter_spacing(style.letter_spacing / font_size);
        let content = if style.content.is_empty() {
            " "
        } else {
            style.content.as_str()
        };
        // Caption emphasis: the highlighted words take the active colour as
        // rich-text spans; the box behind the active word is drawn below.
        let spans = word_spans(content);
        let highlighted: Vec<usize> = style
            .highlight
            .as_ref()
            .map(TextHighlight::highlighted_indices)
            .unwrap_or_default();
        if let Some(highlight) = style.highlight.as_ref().filter(|_| !highlighted.is_empty()) {
            let active = to_color(highlight.active_color);
            let mut rich: Vec<(&str, Attrs)> = Vec::new();
            for (line_index, line) in content.split('\n').enumerate() {
                if line_index > 0 {
                    rich.push(("\n", attrs.clone()));
                }
                let mut cursor = 0;
                for (word_index, span) in spans.iter().enumerate().filter(|(_, span)| span.line == line_index) {
                    if span.start > cursor {
                        rich.push((&line[cursor..span.start], attrs.clone()));
                    }
                    let word_attrs = if highlighted.contains(&word_index) {
                        attrs.clone().color(active)
                    } else {
                        attrs.clone()
                    };
                    rich.push((&line[span.start..span.end], word_attrs));
                    cursor = span.end;
                }
                if cursor < line.len() {
                    rich.push((&line[cursor..], attrs.clone()));
                }
            }
            buffer.set_rich_text(&mut self.font_system, rich, &attrs, Shaping::Advanced, None);
        } else {
            buffer.set_text(&mut self.font_system, content, &attrs, Shaping::Advanced);
        }
        let align = match style.align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        };
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Size the box to the widest line so alignment is relative to the
        // text itself, not to the whole canvas.
        let (text_width, text_height) = measure(&buffer);
        let text_width = text_width.ceil().max(1.0);
        buffer.set_size(&mut self.font_system, Some(text_width), None);
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(&mut self.font_system, false);
        let text_height = {
            let (_, measured) = measure(&buffer);
            measured.max(text_height).ceil().max(1.0)
        };

        // Underline or strike-through per line, placed as the web app does:
        // the underline just under the lowest glyph pixel, the strike
        // through the middle of the glyphs' extent (measured from the em
        // middle). (x, top, width) in text-block pixels.
        let thickness = (font_size * 0.07).max(1.0);
        let mut decorations: Vec<(f32, f32, f32)> = Vec::new();
        let mut decoration_bottom = text_height;
        if style.decoration != TextDecoration::None {
            for run in buffer.layout_runs() {
                if run.glyphs.is_empty() {
                    continue;
                }
                let x_start = run.glyphs.iter().map(|glyph| glyph.x).fold(f32::INFINITY, f32::min);
                let (mut ascent, mut descent, mut middle) = (0.0f32, 0.0f32, 0.0f32);
                for glyph in run.glyphs {
                    let physical = glyph.physical((0.0, 0.0), 1.0);
                    if let Some(image) = self.cache.get_image(&mut self.font_system, physical.cache_key) {
                        ascent = ascent.max(image.placement.top as f32);
                        descent = descent.max(image.placement.height as f32 - image.placement.top as f32);
                    }
                    if let Some(font) = self.font_system.get_font(glyph.font_id) {
                        let metrics = font.as_swash().metrics(&[]).scale(glyph.font_size);
                        middle = middle.max((metrics.ascent - metrics.descent) / 2.0);
                    }
                }
                let top = match style.decoration {
                    TextDecoration::Underline => run.line_y + descent + thickness,
                    TextDecoration::LineThrough => run.line_y - 0.3 * middle - 0.35 * (ascent - descent),
                    TextDecoration::None => continue,
                };
                decoration_bottom = decoration_bottom.max(top + thickness);
                decorations.push((x_start, top, run.line_w));
            }
        }

        // The box behind the word being spoken, in text-block coordinates:
        // its glyphs' horizontal extent padded, one em tall, centred on
        // the line.
        let highlight_box: Option<((f32, f32, f32, f32), [f32; 4], f32)> = style
            .highlight
            .as_ref()
            .filter(|highlight| highlight.mode == HighlightMode::Box)
            .and_then(|highlight| {
                let span = spans.get(highlight.active_index?)?;
                let (mut left, mut right, mut line_top, mut line_height) =
                    (f32::INFINITY, f32::NEG_INFINITY, 0.0f32, 0.0f32);
                for run in buffer.layout_runs() {
                    if run.line_i != span.line {
                        continue;
                    }
                    for glyph in run.glyphs {
                        if glyph.start >= span.start && glyph.end <= span.end {
                            left = left.min(glyph.x);
                            right = right.max(glyph.x + glyph.w);
                            line_top = run.line_top;
                            line_height = run.line_height;
                        }
                    }
                }
                if !left.is_finite() || right <= left {
                    return None;
                }
                let pad_x = font_size * HIGHLIGHT_BOX_PADDING_X;
                let pad_y = font_size * HIGHLIGHT_BOX_PADDING_Y;
                let box_height = font_size * HIGHLIGHT_BOX_HEIGHT + 2.0 * pad_y;
                let top = line_top + line_height / 2.0 - box_height / 2.0;
                Some((
                    (left - pad_x, top, right - left + 2.0 * pad_x, box_height),
                    highlight.box_color,
                    font_size * HIGHLIGHT_BOX_RADIUS,
                ))
            });

        // The background box around the text, in text-block coordinates
        // (the block's top-left is the origin).
        let background_box = style.background.as_ref().map(|background| {
            let (padding_x, padding_y) = (background.padding_x.max(0.0), background.padding_y.max(0.0));
            (
                -padding_x + background.offset_x,
                -padding_y + background.offset_y,
                text_width + 2.0 * padding_x,
                text_height + 2.0 * padding_y,
            )
        });
        // Half extents from the text's centre that hold both the text and
        // the box, so the raster stays centred on the text.
        let (mut half_width, mut half_height) = (text_width / 2.0, text_height / 2.0);
        half_height = half_height.max(decoration_bottom - text_height / 2.0);
        if let Some((left, top, box_width, box_height)) = background_box {
            let (cx, cy) = (text_width / 2.0, text_height / 2.0);
            half_width = half_width.max((left - cx).abs()).max((left + box_width - cx).abs());
            half_height = half_height.max((top - cy).abs()).max((top + box_height - cy).abs());
        }
        if let Some(((left, top, box_width, box_height), _, _)) = highlight_box {
            let (cx, cy) = (text_width / 2.0, text_height / 2.0);
            half_width = half_width.max((left - cx).abs()).max((left + box_width - cx).abs());
            half_height = half_height.max((top - cy).abs()).max((top + box_height - cy).abs());
        }
        // Bounded so a wild size cannot ask for gigabytes; glyphs past the
        // edge are simply cut.
        const MAX_SIDE: f32 = 16_384.0;
        let width = (2.0 * half_width).ceil().clamp(1.0, MAX_SIDE) as u32;
        let height = (2.0 * half_height).ceil().clamp(1.0, MAX_SIDE) as u32;
        let origin_x = width as f32 / 2.0 - text_width / 2.0;
        let origin_y = height as f32 / 2.0 - text_height / 2.0;
        let mut bgra = vec![0u8; (width * height * 4) as usize];

        if let (Some(background), Some((left, top, box_width, box_height))) =
            (&style.background, background_box)
        {
            let radius = (box_width.min(box_height) / 2.0) * (background.corner_radius.clamp(0.0, 100.0) / 100.0);
            fill_rounded_rect(
                &mut bgra,
                width,
                height,
                (origin_x + left, origin_y + top, box_width, box_height),
                background.color,
                radius,
            );
        }

        if let Some(((left, top, box_width, box_height), box_color, radius)) = highlight_box {
            fill_rounded_rect(
                &mut bgra,
                width,
                height,
                (origin_x + left, origin_y + top, box_width, box_height),
                box_color,
                radius,
            );
        }

        let color = to_color(style.color);
        let offset_x = origin_x.round() as i32;
        let offset_y = origin_y.round() as i32;
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

        for (x, top, line_width) in decorations {
            let x0 = (origin_x + x).round().max(0.0) as u32;
            let x1 = ((origin_x + x + line_width).round().max(0.0) as u32).min(width);
            let y0 = (origin_y + top).round().max(0.0) as u32;
            let y1 = ((origin_y + top + thickness).round().max(0.0) as u32).min(height);
            for py in y0..y1 {
                for px in x0..x1 {
                    let index = ((py * width + px) * 4) as usize;
                    blend_over(&mut bgra[index..index + 4], color);
                }
            }
        }

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

/// Fills `rect` (x, y, width, height in raster pixels) with a rounded box.
fn fill_rounded_rect(
    bgra: &mut [u8],
    width: u32,
    height: u32,
    rect: (f32, f32, f32, f32),
    color: [f32; 4],
    radius: f32,
) {
    let (left, top, box_width, box_height) = rect;
    let radius = radius.min(box_width / 2.0).min(box_height / 2.0).max(0.0);
    let pixel = [
        to_byte(color[2]),
        to_byte(color[1]),
        to_byte(color[0]),
        to_byte(color[3]),
    ];
    let x0 = left.floor().max(0.0) as u32;
    let y0 = top.floor().max(0.0) as u32;
    let x1 = ((left + box_width).ceil().max(0.0) as u32).min(width);
    let y1 = ((top + box_height).ceil().max(0.0) as u32).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let coverage = rounded_rect_coverage(
                x as f32 + 0.5 - left,
                y as f32 + 0.5 - top,
                box_width,
                box_height,
                radius,
            );
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
    if x < 0.0 || y < 0.0 || x > width || y > height {
        return 0.0;
    }
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
            decoration: TextDecoration::None,
            background: None,
            highlight: None,
        }
    }

    /// Columns (x) holding a pixel close to `bgr` with alpha above 200.
    fn columns_of(raster: &TextRaster, bgr: [u8; 3]) -> Vec<u32> {
        let close = |a: u8, b: u8| (a as i16 - b as i16).abs() <= 40;
        (0..raster.width)
            .filter(|&x| {
                (0..raster.height).any(|y| {
                    let i = ((y * raster.width + x) * 4) as usize;
                    let px = &raster.bgra[i..i + 4];
                    px[3] > 200 && close(px[0], bgr[0]) && close(px[1], bgr[1]) && close(px[2], bgr[2])
                })
            })
            .collect()
    }

    #[test]
    fn highlights_the_active_word_in_colour_and_boxes_it() {
        let mut rasterizer = TextRasterizer::new();
        let plain = rasterizer.rasterize(&style("one two three"));
        assert!(columns_of(&plain, [0, 255, 0]).is_empty(), "plain text has no green");

        // "two" in green, the rest red: the green columns sit between red ones.
        let mut worded = style("one two three");
        worded.highlight = Some(TextHighlight {
            mode: HighlightMode::Word,
            active_color: [0.0, 1.0, 0.0, 1.0],
            box_color: [0.0, 0.0, 1.0, 1.0],
            active_index: Some(1),
            spoken_count: 2,
        });
        let raster = rasterizer.rasterize(&worded);
        assert_eq!((raster.width, raster.height), (plain.width, plain.height), "colour does not change the layout");
        let green = columns_of(&raster, [0, 255, 0]);
        let red = columns_of(&raster, [0, 0, 255]);
        assert!(!green.is_empty() && !red.is_empty());
        let (g0, g1) = (*green.first().unwrap(), *green.last().unwrap());
        assert!(red.iter().any(|&x| x < g0) && red.iter().any(|&x| x > g1), "green word sits between red words");
        assert!(g1 - g0 < raster.width / 2, "only one word is green");

        // `fill` colours "one two" and leaves "three" red.
        worded.highlight.as_mut().unwrap().mode = HighlightMode::Fill;
        let filled = rasterizer.rasterize(&worded);
        let green = columns_of(&filled, [0, 255, 0]);
        let red = columns_of(&filled, [0, 0, 255]);
        assert!(green.iter().all(|&x| red.iter().all(|&r| r > x)), "red only to the right of the green words");

        // `box` draws a blue box behind "two" that is wider than the word.
        worded.highlight.as_mut().unwrap().mode = HighlightMode::Box;
        let boxed = rasterizer.rasterize(&worded);
        let blue = columns_of(&boxed, [255, 0, 0]);
        let green = columns_of(&boxed, [0, 255, 0]);
        assert!(!blue.is_empty(), "a box was drawn");
        assert!(*blue.first().unwrap() < *green.first().unwrap() && *blue.last().unwrap() > *green.last().unwrap());
        assert_eq!(boxed.width, plain.width, "the box stays within the text block");
        assert!(boxed.height >= plain.height);
    }

    /// Opaque pixels in row `y`.
    fn opaque_in_row(raster: &TextRaster, y: u32) -> usize {
        (0..raster.width)
            .filter(|&x| raster.bgra[((y * raster.width + x) * 4 + 3) as usize] > 128)
            .count()
    }

    #[test]
    fn draws_underline_and_strike_through() {
        let mut rasterizer = TextRasterizer::new();
        let plain = rasterizer.rasterize(&style("Hello"));
        let mut underlined = style("Hello");
        underlined.decoration = TextDecoration::Underline;
        let underlined = rasterizer.rasterize(&underlined);
        // The underline is a full-width bar below the lowest glyph row
        // (within the line-height block for a 1.2 line height).
        let last_glyph_row = (0..plain.height).rev().find(|&y| opaque_in_row(&plain, y) > 0).unwrap();
        let (bar_row, bar) = (0..underlined.height)
            .map(|y| (y, opaque_in_row(&underlined, y)))
            .max_by_key(|&(_, count)| count)
            .unwrap();
        assert!(bar as f32 > underlined.width as f32 * 0.8, "underline covers {bar} of {}", underlined.width);
        assert!(bar_row > last_glyph_row, "underline row {bar_row} is below the glyphs ({last_glyph_row})");
        let plain_widest = (0..plain.height).map(|y| opaque_in_row(&plain, y)).max().unwrap();
        assert!(plain_widest < plain.width as usize * 3 / 4, "plain text has no full-width row: {plain_widest}");

        let mut struck = style("Hello");
        struck.decoration = TextDecoration::LineThrough;
        let struck = rasterizer.rasterize(&struck);
        assert_eq!(struck.height, plain.height, "a strike stays within the block");
        let (strike_row, widest) = (0..struck.height)
            .map(|y| (y, opaque_in_row(&struck, y)))
            .max_by_key(|&(_, count)| count)
            .unwrap();
        assert!(widest as f32 > struck.width as f32 * 0.8, "strike covers {widest} of {}", struck.width);
        assert!(strike_row < last_glyph_row && strike_row > last_glyph_row / 2, "strike row {strike_row} crosses the glyphs");
    }

    #[test]
    fn rasterises_text_with_the_requested_color() {
        let mut rasterizer = TextRasterizer::new();
        let raster = rasterizer.rasterize(&style("Hello"));
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
    fn letter_spacing_is_in_pixels() {
        let mut rasterizer = TextRasterizer::new();
        let plain = rasterizer.rasterize(&style("OPENCUT"));
        let mut spaced = style("OPENCUT");
        spaced.letter_spacing = 4.0;
        let spaced = rasterizer.rasterize(&spaced);
        // Six gaps of 4 px (the last glyph may carry one more).
        let extra = spaced.width as i64 - plain.width as i64;
        assert!((20..=34).contains(&extra), "spaced {} vs plain {}", spaced.width, plain.width);
    }

    #[test]
    fn breaks_lines_only_at_newlines_and_adds_a_background() {
        let mut rasterizer = TextRasterizer::new();
        let one_line = rasterizer.rasterize(&style("one two three four five six seven"));
        let two_lines = rasterizer.rasterize(&style("one\ntwo"));
        assert!(two_lines.height > one_line.height * 3 / 2, "{} vs {}", two_lines.height, one_line.height);
        assert!(two_lines.width < one_line.width);

        let mut boxed = style("Hi");
        boxed.background = Some(TextBackground {
            color: [0.0, 0.0, 1.0, 1.0],
            corner_radius: 50.0,
            padding_x: 10.0,
            padding_y: 6.0,
            offset_x: 0.0,
            offset_y: 0.0,
        });
        let raster = rasterizer.rasterize(&boxed);
        let plain = rasterizer.rasterize(&style("Hi"));
        assert_eq!(raster.width, plain.width + 20);
        assert_eq!(raster.height, plain.height + 12);
        // Corner pixel is outside the rounded corner; the edge midpoint is blue.
        assert_eq!(raster.bgra[3], 0);
        let mid = ((raster.height / 2) * raster.width * 4) as usize;
        assert_eq!(&raster.bgra[mid..mid + 4], &[255, 0, 0, 255]);

        // A shifted box grows the raster on both sides, keeping the text centred.
        let mut shifted = boxed.clone();
        shifted.background.as_mut().unwrap().offset_x = 10.0;
        let raster = rasterizer.rasterize(&shifted);
        assert_eq!(raster.width, plain.width + 40);
        assert_eq!(raster.height, plain.height + 12);
        let left_mid = ((raster.height / 2) * raster.width * 4) as usize;
        assert_eq!(raster.bgra[left_mid + 3], 0, "no box at the far left");
        let right_mid = left_mid + ((raster.width - 1) * 4) as usize;
        assert_eq!(&raster.bgra[right_mid..right_mid + 4], &[255, 0, 0, 255], "box reaches the right edge");
    }
}

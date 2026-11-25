//! Freehand drawing on the video: strokes made with a pen, a marker or a
//! highlighter, each a run of points with pressure and the time they were
//! drawn. A drawing is stored on its element as JSON (`strokes` param) and
//! rasterised here, the same by the desktop and (through WASM) the web.
//! With a reveal time it is drawn on: only what had been drawn by then.
//!
//! Points are in canvas units (0 to 1 across and down), widths a share of
//! the canvas height, so a drawing keeps its place and weight at any size.
//! Each stroke is anti-aliased (one pixel) as round-capped segments whose
//! radius follows the pressure (pen) and composited over the strokes before
//! it; a stroke never darkens where it crosses itself (coverage is the
//! most of its segments), so a highlighter stays even.
//!
//! `fixtures/strokes/cases.json` (an independent Python reference) checks
//! the parsing, the reveal and the pixels.

use serde::{Deserialize, Serialize};

/// What a stroke was drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    /// Thin, its width following the pressure.
    Pen,
    /// Even width, solid.
    Marker,
    /// Even width, see-through.
    Highlighter,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Pen, Tool::Marker, Tool::Highlighter];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Marker => "marker",
            Tool::Highlighter => "highlighter",
        }
    }

    pub fn parse(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|tool| tool.id() == id)
    }

    /// How opaque the tool's ink is.
    pub fn opacity(self) -> f64 {
        match self {
            Tool::Highlighter => 0.4,
            _ => 1.0,
        }
    }

    /// The width a new stroke starts with (share of the canvas height).
    pub fn default_width(self) -> f64 {
        match self {
            Tool::Pen => 0.006,
            Tool::Marker => 0.012,
            Tool::Highlighter => 0.035,
        }
    }

    /// Radius (pixels) at a point of this pressure for a stroke `width`
    /// pixels wide: the pen thins with light pressure, the others do not.
    pub fn radius(self, width: f64, pressure: f64) -> f64 {
        let half = width / 2.0;
        match self {
            Tool::Pen => half * (0.35 + 0.65 * pressure.clamp(0.0, 1.0)),
            _ => half,
        }
    }
}

/// One point of a stroke: where (canvas units), how hard and when (seconds
/// since the drawing began).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub pressure: f64,
    pub time: f64,
}

impl Serialize for Point {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        [round4(self.x), round4(self.y), round4(self.pressure), round4(self.time)].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<f64>::deserialize(deserializer)?;
        if values.len() < 2 || values.iter().any(|value| !value.is_finite()) {
            return Err(serde::de::Error::custom("a point is [x, y, pressure, time]"));
        }
        Ok(Point {
            x: values[0],
            y: values[1],
            pressure: values.get(2).copied().unwrap_or(0.5).clamp(0.0, 1.0),
            time: values.get(3).copied().unwrap_or(0.0).max(0.0),
        })
    }
}

fn round4(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}

/// One stroke.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub tool: Tool,
    /// `#rrggbb`.
    pub color: String,
    /// Share of the canvas height.
    pub width: f64,
    pub points: Vec<Point>,
}

/// A drawing: its strokes in the order they were drawn.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub strokes: Vec<Stroke>,
}

/// The most widths may be (share of the canvas height).
pub const MAX_WIDTH: f64 = 0.2;

impl Drawing {
    /// Reads a drawing as stored; `None` when it is not one. Empty text is
    /// an empty drawing.
    pub fn parse(text: &str) -> Option<Drawing> {
        if text.trim().is_empty() {
            return Some(Drawing::default());
        }
        let mut drawing: Drawing = serde_json::from_str(text).ok()?;
        for stroke in &mut drawing.strokes {
            stroke.width = if stroke.width.is_finite() { stroke.width.clamp(0.0005, MAX_WIDTH) } else { stroke.tool.default_width() };
            if parse_hex(&stroke.color).is_none() {
                stroke.color = "#ffffff".to_owned();
            }
        }
        Some(drawing)
    }

    /// The drawing as stored on its element.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("a drawing serialises")
    }

    /// When the last point was drawn (seconds).
    pub fn duration(&self) -> f64 {
        self.strokes.iter().flat_map(|stroke| stroke.points.iter()).map(|point| point.time).fold(0.0, f64::max)
    }

    /// A stroke added after the others: its times start where the drawing
    /// ended (plus a short pause), so drawing on plays them in order.
    pub fn push_stroke(&mut self, mut stroke: Stroke) {
        let start = if self.strokes.is_empty() { 0.0 } else { self.duration() + STROKE_PAUSE };
        let first = stroke.points.first().map_or(0.0, |point| point.time);
        for point in &mut stroke.points {
            point.time = round4(start + (point.time - first).max(0.0));
        }
        self.strokes.push(stroke);
    }
}

/// The pause between strokes when they are drawn on (seconds).
pub const STROKE_PAUSE: f64 = 0.15;

/// How far drawing on has got, 0 to 1: everything (1) when `draw_on` is 0,
/// else the share of `draw_on` seconds that `elapsed` covers.
pub fn reveal_progress(draw_on: f64, elapsed: f64) -> f64 {
    if !(draw_on > 0.0) || !elapsed.is_finite() {
        return 1.0;
    }
    (elapsed / draw_on).clamp(0.0, 1.0)
}

fn parse_hex(text: &str) -> Option<[f64; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok().map(|v| v as f64 / 255.0);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// The points of a stroke drawn by `until` seconds: those before, and one
/// between the last of them and the next (where the pen was at `until`).
fn revealed(points: &[Point], until: Option<f64>) -> Vec<Point> {
    let Some(until) = until else { return points.to_vec() };
    let mut out = Vec::new();
    for (index, point) in points.iter().enumerate() {
        if point.time <= until {
            out.push(*point);
            continue;
        }
        if let Some(previous) = index.checked_sub(1).map(|i| points[i]).filter(|p| p.time <= until) {
            let span = point.time - previous.time;
            let t = if span > 0.0 { (until - previous.time) / span } else { 0.0 };
            out.push(Point {
                x: previous.x + (point.x - previous.x) * t,
                y: previous.y + (point.y - previous.y) * t,
                pressure: previous.pressure + (point.pressure - previous.pressure) * t,
                time: until,
            });
        }
        break;
    }
    out
}

/// Rasterises a drawing to `width` x `height` straight-alpha RGBA,
/// transparent where nothing is drawn. `progress` (0 to 1) draws it on:
/// only what was drawn in that share of its duration.
pub fn rasterise(drawing: &Drawing, width: usize, height: usize, progress: f64) -> Vec<u8> {
    let mut colour = vec![[0.0f64; 3]; width * height];
    let mut alpha = vec![0.0f64; width * height];
    let until = if progress >= 1.0 { None } else { Some(drawing.duration() * progress.max(0.0)) };
    let (w, h) = (width as f64, height as f64);
    let mut coverage = vec![0.0f64; width * height];
    for stroke in &drawing.strokes {
        let points = revealed(&stroke.points, until);
        if points.is_empty() {
            continue;
        }
        let Some(rgb) = parse_hex(&stroke.color) else { continue };
        let stroke_width = stroke.width.clamp(0.0005, MAX_WIDTH) * h;
        let opacity = stroke.tool.opacity();
        let placed: Vec<(f64, f64, f64)> = points.iter().map(|p| (p.x * w, p.y * h, stroke.tool.radius(stroke_width, p.pressure))).collect();
        // The stroke's bounds, a pixel wider for the soft edge.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y, r) in &placed {
            x0 = x0.min(x - r - 1.0);
            y0 = y0.min(y - r - 1.0);
            x1 = x1.max(x + r + 1.0);
            y1 = y1.max(y + r + 1.0);
        }
        let (px0, py0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
        let (px1, py1) = ((x1.ceil().max(0.0) as usize).min(width), (y1.ceil().max(0.0) as usize).min(height));
        if px0 >= px1 || py0 >= py1 {
            continue;
        }
        for row in py0..py1 {
            coverage[row * width + px0..row * width + px1].fill(0.0);
        }
        let segments: Vec<((f64, f64, f64), (f64, f64, f64))> = if placed.len() == 1 {
            vec![(placed[0], placed[0])]
        } else {
            placed.windows(2).map(|pair| (pair[0], pair[1])).collect()
        };
        for (a, b) in segments {
            let sx0 = ((a.0.min(b.0) - a.2.max(b.2) - 1.0).floor().max(px0 as f64)) as usize;
            let sy0 = ((a.1.min(b.1) - a.2.max(b.2) - 1.0).floor().max(py0 as f64)) as usize;
            let sx1 = ((a.0.max(b.0) + a.2.max(b.2) + 1.0).ceil().min(px1 as f64)).max(sx0 as f64) as usize;
            let sy1 = ((a.1.max(b.1) + a.2.max(b.2) + 1.0).ceil().min(py1 as f64)).max(sy0 as f64) as usize;
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length2 = dx * dx + dy * dy;
            for row in sy0..sy1 {
                let qy = row as f64 + 0.5;
                for column in sx0..sx1 {
                    let qx = column as f64 + 0.5;
                    let t = if length2 > 0.0 { (((qx - a.0) * dx + (qy - a.1) * dy) / length2).clamp(0.0, 1.0) } else { 0.0 };
                    let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
                    let radius = a.2 + (b.2 - a.2) * t;
                    let distance = ((qx - cx) * (qx - cx) + (qy - cy) * (qy - cy)).sqrt();
                    let cover = (radius + 0.5 - distance).clamp(0.0, 1.0);
                    let index = row * width + column;
                    if cover > coverage[index] {
                        coverage[index] = cover;
                    }
                }
            }
        }
        for row in py0..py1 {
            for column in px0..px1 {
                let index = row * width + column;
                let source = coverage[index] * opacity;
                if source <= 0.0 {
                    continue;
                }
                let below = alpha[index];
                let out = source + below * (1.0 - source);
                for channel in 0..3 {
                    colour[index][channel] = (rgb[channel] * source + colour[index][channel] * below * (1.0 - source)) / out;
                }
                alpha[index] = out;
            }
        }
    }
    let mut rgba = vec![0u8; width * height * 4];
    for index in 0..width * height {
        if alpha[index] <= 0.0 {
            continue;
        }
        for channel in 0..3 {
            rgba[index * 4 + channel] = (colour[index][channel] * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
        }
        rgba[index * 4 + 3] = (alpha[index] * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    rgba
}

/// Size a drawing is rasterised at for a canvas: the canvas, scaled down so
/// its longer side is at most `max_side` (the layer is then drawn at the
/// canvas size).
pub fn raster_size(canvas_width: u32, canvas_height: u32, max_side: u32) -> (u32, u32) {
    let longest = canvas_width.max(canvas_height).max(1);
    if longest <= max_side {
        return (canvas_width.max(1), canvas_height.max(1));
    }
    let scale = max_side as f64 / longest as f64;
    (((canvas_width as f64 * scale).round() as u32).max(1), ((canvas_height as f64 * scale).round() as u32).max(1))
}

/// The longest side drawings are rasterised at.
pub const MAX_RASTER_SIDE: u32 = 1920;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/strokes/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn rasterises_like_the_reference() {
        let fixture = fixture();
        for case in fixture["cases"].as_array().unwrap() {
            let drawing = Drawing::parse(case["drawing"].as_str().unwrap()).expect("parses");
            let (width, height) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
            let progress = case["progress"].as_f64().unwrap();
            let got = rasterise(&drawing, width, height, progress);
            let want: Vec<u8> = case["rgba"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let worst = got.iter().zip(&want).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap_or(0);
            assert!(worst <= 1, "{}: off by {worst}", case["name"]);
            assert!((drawing.duration() - case["duration"].as_f64().unwrap()).abs() < 1e-9, "{}", case["name"]);
        }
    }

    #[test]
    fn strokes_round_trip_and_follow_each_other_in_time() {
        let mut drawing = Drawing::default();
        let stroke = |t0: f64| Stroke {
            tool: Tool::Pen,
            color: "#ff3b30".to_owned(),
            width: 0.01,
            points: vec![
                Point { x: 0.1, y: 0.2, pressure: 0.5, time: t0 },
                Point { x: 0.3, y: 0.25, pressure: 0.8, time: t0 + 0.4 },
            ],
        };
        drawing.push_stroke(stroke(12.0));
        drawing.push_stroke(stroke(30.0));
        assert_eq!(drawing.strokes[0].points[0].time, 0.0);
        assert_eq!(drawing.strokes[1].points[0].time, 0.55, "after the first stroke and a pause");
        assert!((drawing.duration() - 0.95).abs() < 1e-9);
        let text = drawing.encode();
        assert_eq!(Drawing::parse(&text), Some(drawing));
        assert_eq!(Drawing::parse(""), Some(Drawing::default()));
        assert_eq!(Drawing::parse("not json"), None);
        assert_eq!(reveal_progress(0.0, 3.0), 1.0);
        assert_eq!(reveal_progress(2.0, 1.0), 0.5);
        assert_eq!(reveal_progress(2.0, 9.0), 1.0);
        assert_eq!(raster_size(3840, 2160, 1920), (1920, 1080));
        assert_eq!(raster_size(1080, 1920, 1920), (1080, 1920));
        for tool in Tool::ALL {
            assert_eq!(Tool::parse(tool.id()), Some(tool));
        }
    }

    #[test]
    fn nothing_is_drawn_before_the_first_stroke_starts() {
        let drawing = Drawing {
            strokes: vec![Stroke {
                tool: Tool::Marker,
                color: "#00ff00".to_owned(),
                width: 0.1,
                points: vec![Point { x: 0.5, y: 0.5, pressure: 1.0, time: 0.0 }, Point { x: 0.9, y: 0.5, pressure: 1.0, time: 1.0 }],
            }],
        };
        let start = rasterise(&drawing, 40, 20, 0.0);
        let half = rasterise(&drawing, 40, 20, 0.5);
        let full = rasterise(&drawing, 40, 20, 1.0);
        let lit = |rgba: &[u8]| rgba.chunks(4).filter(|p| p[3] > 0).count();
        assert!(lit(&start) > 0, "the pen is down at the first point");
        assert!(lit(&start) < lit(&half) && lit(&half) < lit(&full));
    }
}

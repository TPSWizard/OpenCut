//! Lottie animations for both editors: a Lottie JSON file or a dotLottie
//! archive is read and checked here, then drawn frame by frame on the CPU
//! with [velato](https://github.com/linebender/velato) (Lottie to vector
//! commands) and [vello_cpu](https://github.com/linebender/vello) (the
//! rasteriser). The desktop links this crate directly and the web editor
//! uses it through `opencut-wasm`, so both draw the same pixels.
//!
//! Files come from users, so they are checked before anything is drawn:
//! size, canvas, frame range, repeater copies and precomposition nesting
//! all have limits (see the `MAX_*` constants). velato does not draw text
//! layers, embedded images, time remapping, dashes or layer effects; the
//! rest of the animation still plays.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use velato::{Composition, RenderSink, Renderer, model::fixed};
use vello_cpu::kurbo::{Affine, BezPath, Shape};
use vello_cpu::peniko::{self, BlendMode, Fill, ImageAlphaType};
use vello_cpu::{Pixmap, RenderContext, Resources};

/// Largest file read, Lottie JSON or dotLottie archive.
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
/// Largest animation JSON inside a dotLottie archive.
pub const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
/// Largest composition side, in Lottie pixels.
pub const MAX_SIDE: f64 = 8192.0;
/// Largest side drawn in one frame.
pub const MAX_RENDER_SIDE: u32 = 4096;
/// Longest animation, in seconds.
pub const MAX_DURATION_SECONDS: f64 = 3600.0;
/// Most shapes one repeater chain may produce (copies multiplied through nesting).
pub const MAX_REPEATER_COPIES: f64 = 1000.0;
/// Deepest precomposition nesting.
pub const MAX_PRECOMP_DEPTH: usize = 16;
/// Path flattening tolerance for shapes velato hands over as curves.
const TOLERANCE: f64 = 0.1;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LottieError {
    #[error("The file is larger than {} MB", MAX_FILE_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("Not a Lottie animation: {0}")]
    NotLottie(String),
    #[error("The dotLottie archive cannot be read: {0}")]
    Archive(String),
    #[error("The animation is outside the limits: {0}")]
    Limits(String),
}

/// Size and timing of an animation, in the file's own units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LottieInfo {
    pub width: u32,
    pub height: u32,
    /// Lottie frames per second (`fr`).
    pub frame_rate: f64,
    /// First frame (`ip`).
    pub in_frame: f64,
    /// Frame where the animation ends (`op`), not drawn itself.
    pub out_frame: f64,
}

impl LottieInfo {
    pub fn frames(&self) -> f64 {
        self.out_frame - self.in_frame
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() / self.frame_rate
    }

    /// The Lottie frame shown `seconds` into the clip; the animation loops.
    pub fn frame_at(&self, seconds: f64) -> f64 {
        let frames = self.frames();
        let elapsed = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 } * self.frame_rate;
        self.in_frame + elapsed.rem_euclid(frames)
    }
}

/// Whether `bytes` look like a Lottie file: a dotLottie archive, or JSON
/// with Lottie's top-level fields. Cheap enough for sorting imports.
pub fn is_lottie(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"PK\x03\x04") {
        return animation_json(bytes).is_ok();
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    check_top_level(&value).is_ok()
}

/// A parsed, checked animation that can draw any of its frames.
pub struct Animation {
    composition: Composition,
    info: LottieInfo,
    renderer: Renderer,
    context: Option<RenderContext>,
    resources: Resources,
}

impl std::fmt::Debug for Animation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Animation").field("info", &self.info).finish_non_exhaustive()
    }
}

impl Animation {
    /// Reads a Lottie JSON file or a dotLottie archive (its first animation).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LottieError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(LottieError::TooLarge);
        }
        let json = if bytes.starts_with(b"PK\x03\x04") { animation_json(bytes)? } else { bytes.to_vec() };
        let value: serde_json::Value =
            serde_json::from_slice(&json).map_err(|error| LottieError::NotLottie(error.to_string()))?;
        let info = check_top_level(&value)?;
        check_repeaters(&value)?;
        check_precomps(&value)?;
        let composition = Composition::from_json(value).map_err(|error| LottieError::NotLottie(error.to_string()))?;
        Ok(Self { composition, info, renderer: Renderer::new(), context: None, resources: Resources::new() })
    }

    pub fn info(&self) -> LottieInfo {
        self.info
    }

    /// Draws `frame` scaled to `width` × `height`; straight-alpha RGBA,
    /// row by row. Sides are clamped to 1 ..= [`MAX_RENDER_SIDE`].
    pub fn render_rgba(&mut self, frame: f64, width: u32, height: u32) -> Vec<u8> {
        let width = width.clamp(1, MAX_RENDER_SIDE) as u16;
        let height = height.clamp(1, MAX_RENDER_SIDE) as u16;
        let context = match self.context.as_mut() {
            Some(context) if context.width() == width && context.height() == height => {
                context.reset();
                context
            }
            _ => self.context.insert(RenderContext::new(width, height)),
        };
        let transform = Affine::scale_non_uniform(
            f64::from(width) / f64::from(self.info.width),
            f64::from(height) / f64::from(self.info.height),
        );
        let frame = if frame.is_finite() { frame.clamp(self.info.in_frame, self.info.out_frame) } else { self.info.in_frame };
        self.renderer.append(&self.composition, frame, transform, 1.0, &mut Sink { context });
        context.flush();
        let mut pixmap = Pixmap::new(width, height);
        context.render(&mut pixmap, &mut self.resources);
        pixmap.take_rgba8(ImageAlphaType::Alpha)
    }
}

/// velato's drawing commands, carried out by vello_cpu.
struct Sink<'a> {
    context: &'a mut RenderContext,
}

impl Sink<'_> {
    fn path(shape: &impl Shape) -> BezPath {
        shape.path_elements(TOLERANCE).collect()
    }
}

impl RenderSink for Sink<'_> {
    fn push_layer(&mut self, blend: impl Into<BlendMode>, alpha: f32, transform: Affine, shape: &impl Shape) {
        self.context.set_transform(transform);
        self.context.set_fill_rule(Fill::NonZero);
        let clip = Self::path(shape);
        self.context.push_layer(Some(&clip), Some(blend.into()), Some(alpha), None, None);
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        self.context.set_transform(transform);
        self.context.set_fill_rule(Fill::NonZero);
        self.context.push_clip_layer(&Self::path(shape));
    }

    fn pop_layer(&mut self) {
        self.context.pop_layer();
    }

    fn draw(&mut self, stroke: Option<&fixed::Stroke>, transform: Affine, brush: &fixed::Brush, shape: &impl Shape) {
        let paint: vello_cpu::PaintType = match brush {
            peniko::Brush::Solid(color) => peniko::Brush::Solid(*color),
            peniko::Brush::Gradient(gradient) => peniko::Brush::Gradient(gradient.clone()),
            // Image brushes do not come from Lottie shapes.
            peniko::Brush::Image(_) => return,
        };
        self.context.set_transform(transform);
        self.context.reset_paint_transform();
        self.context.set_paint(paint);
        let path = Self::path(shape);
        match stroke {
            Some(stroke) => {
                self.context.set_stroke(stroke.clone());
                self.context.stroke_path(&path);
            }
            None => {
                self.context.set_fill_rule(Fill::NonZero);
                self.context.fill_path(&path);
            }
        }
    }

    /// Embedded and linked images are not drawn (velato cannot decode them
    /// here); the rest of the frame is.
    fn draw_image(&mut self, _image: &velato::model::ImageAsset, _transform: Affine, _alpha: f64) {}
}

/// The animation JSON inside a dotLottie archive: the first animation the
/// manifest lists (`a/<id>.json` in version 2, `animations/<id>.json` in
/// version 1), else the first JSON file in either folder.
fn animation_json(bytes: &[u8]) -> Result<Vec<u8>, LottieError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| LottieError::Archive(error.to_string()))?;
    let read = |archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str| -> Option<Result<Vec<u8>, LottieError>> {
        let file = archive.by_name(name).ok()?;
        if file.size() > MAX_JSON_BYTES {
            return Some(Err(LottieError::TooLarge));
        }
        let mut out = Vec::with_capacity(file.size() as usize);
        Some(
            file.take(MAX_JSON_BYTES + 1)
                .read_to_end(&mut out)
                .map_err(|error| LottieError::Archive(error.to_string()))
                .and_then(|_| if out.len() as u64 > MAX_JSON_BYTES { Err(LottieError::TooLarge) } else { Ok(out) }),
        )
    };
    let manifest_id = read(&mut archive, "manifest.json")
        .and_then(Result::ok)
        .and_then(|manifest| serde_json::from_slice::<serde_json::Value>(&manifest).ok())
        .and_then(|manifest| manifest.get("animations")?.get(0)?.get("id")?.as_str().map(str::to_owned));
    if let Some(id) = manifest_id.filter(|id| !id.contains(['/', '\\']) && !id.contains("..")) {
        for name in [format!("a/{id}.json"), format!("animations/{id}.json")] {
            if let Some(json) = read(&mut archive, &name) {
                return json;
            }
        }
    }
    let first = archive
        .file_names()
        .filter_map(|name| name.ok().map(|name| name.into_owned()))
        .filter(|name| (name.starts_with("a/") || name.starts_with("animations/")) && name.ends_with(".json"))
        .min();
    match first {
        Some(name) => read(&mut archive, &name).unwrap_or_else(|| Err(LottieError::Archive("missing animation".into()))),
        None => Err(LottieError::NotLottie("the archive has no animation".into())),
    }
}

fn number(value: &serde_json::Value, key: &str) -> Result<f64, LottieError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| LottieError::NotLottie(format!("no `{key}`")))
}

fn check_top_level(value: &serde_json::Value) -> Result<LottieInfo, LottieError> {
    if !value.is_object() {
        return Err(LottieError::NotLottie("not a JSON object".into()));
    }
    if !value.get("layers").is_some_and(serde_json::Value::is_array) {
        return Err(LottieError::NotLottie("no `layers`".into()));
    }
    let width = number(value, "w")?;
    let height = number(value, "h")?;
    let frame_rate = number(value, "fr")?;
    let in_frame = number(value, "ip")?;
    let out_frame = number(value, "op")?;
    if !(width >= 1.0 && height >= 1.0 && width <= MAX_SIDE && height <= MAX_SIDE) {
        return Err(LottieError::Limits(format!("canvas {width}×{height}; each side must be 1 to {MAX_SIDE}")));
    }
    if !(frame_rate > 0.0 && frame_rate <= 240.0) {
        return Err(LottieError::Limits(format!("{frame_rate} frames per second")));
    }
    if out_frame <= in_frame {
        return Err(LottieError::Limits("it has no frames (op is not after ip)".into()));
    }
    if (out_frame - in_frame) / frame_rate > MAX_DURATION_SECONDS {
        return Err(LottieError::Limits("longer than an hour".into()));
    }
    Ok(LottieInfo { width: width.round() as u32, height: height.round() as u32, frame_rate, in_frame, out_frame })
}

/// The largest value an animated or static number property takes.
fn property_max(property: Option<&serde_json::Value>) -> f64 {
    let Some(keyframes) = property.and_then(|property| property.get("k")) else {
        return 1.0;
    };
    let scalar = |value: &serde_json::Value| match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::Array(items) => items.first().and_then(serde_json::Value::as_f64),
        _ => None,
    };
    match keyframes {
        serde_json::Value::Array(items) if items.first().is_some_and(serde_json::Value::is_object) => items
            .iter()
            .flat_map(|key| [key.get("s"), key.get("e")])
            .flatten()
            .filter_map(scalar)
            .fold(1.0, f64::max),
        other => scalar(other).unwrap_or(1.0),
    }
}

/// Repeaters multiply the shapes after them, and nested ones multiply each
/// other; the product along any chain must stay under the limit.
fn check_repeaters(value: &serde_json::Value) -> Result<(), LottieError> {
    fn walk(value: &serde_json::Value, product: f64) -> Result<(), LottieError> {
        match value {
            serde_json::Value::Array(items) => {
                let mut product = product;
                for item in items {
                    if item.get("ty").and_then(serde_json::Value::as_str) == Some("rp") {
                        let copies = property_max(item.get("c"));
                        if !copies.is_finite() {
                            return Err(LottieError::Limits("a repeater has no copy count".into()));
                        }
                        product *= copies.max(1.0);
                        if product > MAX_REPEATER_COPIES {
                            return Err(LottieError::Limits(format!(
                                "repeaters make more than {MAX_REPEATER_COPIES} copies"
                            )));
                        }
                    }
                    walk(item, product)?;
                }
                Ok(())
            }
            serde_json::Value::Object(map) => map.values().try_for_each(|child| walk(child, product)),
            _ => Ok(()),
        }
    }
    walk(value, 1.0)
}

/// Precompositions may nest, but not loop or nest without end.
fn check_precomps(value: &serde_json::Value) -> Result<(), LottieError> {
    let refs = |layers: Option<&serde_json::Value>| -> Vec<String> {
        layers
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|layer| layer.get("refId").and_then(serde_json::Value::as_str).map(str::to_owned))
            .collect()
    };
    let assets: HashMap<String, Vec<String>> = value
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|asset| Some((asset.get("id")?.as_str()?.to_owned(), refs(asset.get("layers")))))
        .collect();
    /// How deep `id` nests, counting itself; refuses loops.
    fn depth(
        id: &str,
        assets: &HashMap<String, Vec<String>>,
        stack: &mut Vec<String>,
        known: &mut HashMap<String, usize>,
    ) -> Result<usize, LottieError> {
        if stack.iter().any(|open| open == id) {
            return Err(LottieError::Limits(format!("precomposition `{id}` contains itself")));
        }
        if let Some(&depth) = known.get(id) {
            return Ok(depth);
        }
        stack.push(id.to_owned());
        let mut deepest = 0;
        for child in assets.get(id).into_iter().flatten() {
            deepest = deepest.max(depth(child, assets, stack, known)?);
        }
        stack.pop();
        let depth = deepest + 1;
        known.insert(id.to_owned(), depth);
        Ok(depth)
    }
    let mut known = HashMap::new();
    for root in refs(value.get("layers")) {
        if depth(&root, &assets, &mut Vec::new(), &mut known)? > MAX_PRECOMP_DEPTH {
            return Err(LottieError::Limits(format!("precompositions nest deeper than {MAX_PRECOMP_DEPTH}")));
        }
    }
    Ok(())
}

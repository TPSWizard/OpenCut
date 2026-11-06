use std::collections::HashMap;
use std::sync::Arc;

use compositor::{
    BlendMode, CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor,
    LayerDescriptor, LayerMaskDescriptor, QuadTransformDescriptor, TransitionDescriptor,
    TransitionParamDescriptor,
};
use editor::{
    FrameItemPlan, LayerPlan, LayerSource, Mask, MaskPlan, MediaItem, ParamValue, Params, Project, TextStyle,
    fit_contain, plan_frame,
};
use gpu::{GpuContext, wgpu};
use media::{Ffmpeg, VideoSource, decode_image};
use time::MediaTime;

use crate::effects::{backdrop_blur_passes, effect_pass_groups};
use crate::error::RenderError;
use crate::mask::{rasterize_mask, rasterize_mask_stroke, rasterize_text_mask, rasterize_text_mask_stroke};
use crate::text::TextRasterizer;

#[derive(Clone, Debug, Default)]
pub struct RendererOptions {
    /// Without FFmpeg, video elements render as nothing (images and text
    /// still work).
    pub ffmpeg: Option<Arc<Ffmpeg>>,
    /// A directory of font files to add to the system fonts, such as the
    /// desktop's `~/.opencut/fonts`.
    pub fonts_dir: Option<std::path::PathBuf>,
}

/// A composited frame: straight-alpha BGRA, tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedFrame {
    pub time: MediaTime,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

struct TextureSlot {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

pub struct Renderer {
    context: GpuContext,
    compositor: Compositor,
    ffmpeg: Option<Arc<Ffmpeg>>,
    text: TextRasterizer,
    slots: HashMap<String, TextureSlot>,
    videos: HashMap<String, VideoSource>,
    /// Natural size of images already uploaded, by media id.
    images: HashMap<String, (u32, u32)>,
    /// Rasterised text by element id: the style it was made from and its size.
    texts: HashMap<String, (String, u32, u32)>,
    /// Rasterised masks by element id: what they were made from.
    masks: HashMap<String, String>,
    /// Element id → key of the mask stroke texture currently uploaded.
    strokes: HashMap<String, String>,
    fonts_dir: Option<std::path::PathBuf>,
}

impl Renderer {
    /// Acquires a GPU device. Fails where no adapter is available.
    pub fn new(options: RendererOptions) -> Result<Self, RenderError> {
        let context = pollster::block_on(GpuContext::new())?;
        let compositor = Compositor::new(&context);
        let mut text = TextRasterizer::new();
        if let Some(dir) = &options.fonts_dir {
            text.load_fonts_dir(dir);
        }
        Ok(Self {
            context,
            compositor,
            ffmpeg: options.ffmpeg,
            text,
            slots: HashMap::new(),
            videos: HashMap::new(),
            images: HashMap::new(),
            texts: HashMap::new(),
            masks: HashMap::new(),
            strokes: HashMap::new(),
            fonts_dir: options.fonts_dir,
        })
    }

    /// Picks up font files added to the fonts directory since start and
    /// drops rasterised text so it is laid out again with them.
    pub fn reload_fonts(&mut self) {
        if let Some(dir) = self.fonts_dir.clone() {
            self.text.load_fonts_dir(&dir);
        }
        self.texts.clear();
    }

    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some()
    }

    /// Composites the project at `time` into a frame of `width` x `height`
    /// pixels (normally the canvas aspect ratio, at any scale).
    pub fn render(
        &mut self,
        project: &Project,
        time: MediaTime,
        width: u32,
        height: u32,
    ) -> Result<RenderedFrame, RenderError> {
        let width = width.max(2) & !1;
        let height = height.max(2) & !1;
        let plan = plan_frame(project, time);
        let canvas_width = plan.width.max(1) as f32;
        let canvas_height = plan.height.max(1) as f32;
        let scale_x = width as f32 / canvas_width;
        let scale_y = height as f32 / canvas_height;

        let mut items = Vec::with_capacity(plan.items.len() + plan.backdrops.len());
        for backdrop in &plan.backdrops {
            let Some((texture_id, natural_width, natural_height)) =
                self.prepare_source(project, &backdrop.element_id, &backdrop.source, scale_x)?
            else {
                continue;
            };
            // Scaled to cover the frame, centred.
            let cover = (width as f32 / natural_width.max(1) as f32)
                .max(height as f32 / natural_height.max(1) as f32);
            let passes = backdrop_blur_passes(backdrop.blur_intensity as f64, width, height);
            items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                texture_id,
                transform: QuadTransformDescriptor {
                    center_x: width as f32 / 2.0,
                    center_y: height as f32 / 2.0,
                    width: (natural_width as f32 * cover).max(1.0),
                    height: (natural_height as f32 * cover).max(1.0),
                    rotation_degrees: 0.0,
                    flip_x: false,
                    flip_y: false,
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effect_pass_groups: if passes.is_empty() { Vec::new() } else { vec![passes] },
                mask: None,
            }));
        }
        let frame_size = (width, height, canvas_width, canvas_height, scale_x, scale_y);
        for item in &plan.items {
            match item {
                FrameItemPlan::Layer(layer) => {
                    items.extend(self.layer_items(project, layer, frame_size)?.into_iter().map(FrameItemDescriptor::Layer));
                }
                FrameItemPlan::Transition(transition) => {
                    let from = match &transition.from {
                        Some(layer) => self.layer_items(project, layer, frame_size)?,
                        None => Vec::new(),
                    };
                    let to = match &transition.to {
                        Some(layer) => self.layer_items(project, layer, frame_size)?,
                        None => Vec::new(),
                    };
                    items.push(FrameItemDescriptor::Transition(TransitionDescriptor {
                        name: transition.kind.clone(),
                        progress: transition.progress,
                        params: transition_params(&transition.params),
                        from,
                        to,
                    }));
                }
            }
        }

        let frame = FrameDescriptor {
            width,
            height,
            clear: CanvasClearDescriptor {
                color: plan.background,
            },
            items,
        };
        let texture = self.compositor.render_frame_to_texture(&self.context, &frame)?;
        let mut bgra = self.context.read_texture(&texture)?;
        if !self.context.pixels_are_bgra() {
            for pixel in bgra.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok(RenderedFrame {
            time,
            width,
            height,
            bgra,
        })
    }

    /// A planned layer as frame items: the layer itself and, for a mask
    /// with a stroke, the stroke drawn over it. Empty when the source
    /// cannot be shown.
    fn layer_items(
        &mut self,
        project: &Project,
        layer: &LayerPlan,
        (width, height, canvas_width, canvas_height, scale_x, scale_y): (u32, u32, f32, f32, f32, f32),
    ) -> Result<Vec<LayerDescriptor>, RenderError> {
        let Some((texture_id, natural_width, natural_height)) = self.prepare_source(
            project,
            &layer.element_id,
            &layer.source,
            scale_x * layer.transform.scale_x.abs(),
        )?
        else {
            return Ok(Vec::new());
        };
        let (base_width, base_height) = match layer.source {
            LayerSource::Text(_) => (natural_width as f32, natural_height as f32),
            _ => fit_contain(
                natural_width as f32,
                natural_height as f32,
                canvas_width,
                canvas_height,
            ),
        };
        let transform = &layer.transform;
        let quad = QuadTransformDescriptor {
            center_x: (canvas_width / 2.0 + transform.position_x) * scale_x,
            center_y: (canvas_height / 2.0 + transform.position_y) * scale_y,
            width: (base_width * transform.scale_x.abs() * scale_x).max(1.0),
            height: (base_height * transform.scale_y.abs() * scale_y).max(1.0),
            rotation_degrees: transform.rotate_degrees,
            flip_x: transform.scale_x < 0.0,
            flip_y: transform.scale_y < 0.0,
        };
        let (mask, stroke) = match layer.mask.as_ref() {
            Some(mask) => {
                let (mask, stroke) =
                    self.prepare_mask(&layer.element_id, mask, &quad, width, height, scale_x);
                (Some(mask), stroke)
            }
            None => (None, None),
        };
        let mut items = vec![LayerDescriptor {
            texture_id,
            transform: quad,
            opacity: layer.opacity,
            blend_mode: parse_blend_mode(&layer.blend_mode),
            effect_pass_groups: effect_pass_groups(&layer.effects, width, height, layer.local_time.to_seconds_f64()),
            mask,
        }];
        items.extend(stroke);
        Ok(items)
    }

    /// Makes sure the source's texture is uploaded and current. Returns the
    /// texture id and the source's natural size in canvas pixels, or `None`
    /// when it cannot be shown (missing file, no FFmpeg).
    fn prepare_source(
        &mut self,
        project: &Project,
        element_id: &str,
        source: &LayerSource,
        output_scale: f32,
    ) -> Result<Option<(String, u32, u32)>, RenderError> {
        match source {
            LayerSource::Image { media_id } => {
                let media = media_item(project, media_id)?;
                let texture_id = format!("media:{media_id}");
                if let Some(&(width, height)) = self.images.get(media_id) {
                    return Ok(Some((texture_id, width, height)));
                }
                let frame = match decode_image(&media.path) {
                    Ok(frame) => frame,
                    Err(error) => {
                        eprintln!("render: {error}");
                        return Ok(None);
                    }
                };
                self.upload(&texture_id, frame.width, frame.height, &frame.bgra);
                self.images
                    .insert(media_id.clone(), (frame.width, frame.height));
                Ok(Some((texture_id, frame.width, frame.height)))
            }
            LayerSource::Video { media_id, time } => {
                let media = media_item(project, media_id)?;
                let Some(ffmpeg) = self.ffmpeg.clone() else {
                    return Ok(None);
                };
                let natural = (
                    media.width.unwrap_or(1920).max(1),
                    media.height.unwrap_or(1080).max(1),
                );
                let fps = media.fps.filter(|fps| *fps > 0.0).unwrap_or(30.0);
                let canvas = project.settings.canvas;
                let (fit_width, _) = fit_contain(
                    natural.0 as f32,
                    natural.1 as f32,
                    canvas.width as f32,
                    canvas.height as f32,
                );
                let wanted_width = decode_width(fit_width * output_scale, natural.0);
                let needs_new = match self.videos.get(media_id) {
                    Some(source) => {
                        let current = source.output_width() as f32;
                        wanted_width as f32 > current * 1.3 || (wanted_width as f32) < current * 0.6
                    }
                    None => true,
                };
                if needs_new {
                    self.videos.insert(
                        media_id.clone(),
                        VideoSource::new(ffmpeg, &media.path, natural, wanted_width, fps),
                    );
                }
                let source = self.videos.get_mut(media_id).expect("inserted above");
                let frame = match source.frame_at(time.to_seconds_f64()) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => return Ok(None),
                    Err(error) => {
                        eprintln!("render: {error}");
                        self.videos.remove(media_id);
                        return Ok(None);
                    }
                };
                let texture_id = format!("media:{media_id}");
                let (width, height, bgra) = (frame.width, frame.height, frame.bgra.clone());
                self.upload(&texture_id, width, height, &bgra);
                Ok(Some((texture_id, natural.0, natural.1)))
            }
            LayerSource::Text(style) => {
                let texture_id = format!("text:{element_id}");
                let key = format!("{style:?}");
                if let Some((current_key, width, height)) = self.texts.get(element_id)
                    && *current_key == key
                {
                    return Ok(Some((texture_id, *width, *height)));
                }
                let raster = self.text.rasterize(style);
                self.upload(&texture_id, raster.width, raster.height, &raster.bgra);
                self.texts.insert(
                    element_id.to_owned(),
                    (key, raster.width, raster.height),
                );
                Ok(Some((texture_id, raster.width, raster.height)))
            }
        }
    }

    /// Makes sure the element's mask texture matches the mask and the
    /// element's placement in this frame, and describes it for the
    /// compositor, with the mask's stroke (if any) as a full-frame layer to
    /// draw over the element. Feather and stroke width are given in canvas
    /// pixels and scaled to the frame.
    fn prepare_mask(
        &mut self,
        element_id: &str,
        mask: &MaskPlan,
        quad: &QuadTransformDescriptor,
        width: u32,
        height: u32,
        scale: f32,
    ) -> (LayerMaskDescriptor, Option<LayerDescriptor>) {
        let texture_id = format!("mask:{element_id}");
        let key = format!("{}:{:?}:{quad:?}:{width}x{height}", mask.kind, mask.params);
        // A text mask's glyphs, sized against the element's own height like
        // the web app's mask canvas; the mask's params carry the text
        // settings. Rasterised once for the mask and its stroke.
        let mut glyphs = None;
        let mut glyphs_for = |text: &mut TextRasterizer| {
            glyphs
                .get_or_insert_with(|| {
                    let style = TextStyle::from_params(&mask.params, quad.height.max(1.0));
                    text.rasterize(&style)
                })
                .clone()
        };
        if self.masks.get(element_id) != Some(&key) {
            let pixels = if mask.kind == Mask::TEXT {
                let glyphs = glyphs_for(&mut self.text);
                rasterize_text_mask(&mask.params, quad, width, height, &glyphs)
            } else {
                rasterize_mask(&mask.kind, &mask.params, quad, width, height)
            };
            self.upload(&texture_id, width, height, &pixels);
            self.masks.insert(element_id.to_owned(), key);
        }
        let descriptor = LayerMaskDescriptor {
            texture_id,
            feather: (mask.params.number("feather", 0.0).max(0.0) as f32 * scale).max(0.0),
            inverted: mask.params.bool("inverted", false),
        };
        let stroke = if mask.params.number("strokeWidth", 0.0) > 0.0 {
            let stroke_id = format!("mask-stroke:{element_id}");
            let key = format!("{}:{:?}:{quad:?}:{width}x{height}:{scale}", mask.kind, mask.params);
            let uploaded = if self.strokes.get(element_id) == Some(&key) {
                true
            } else if let Some(pixels) = if mask.kind == Mask::TEXT {
                let glyphs = glyphs_for(&mut self.text);
                rasterize_text_mask_stroke(&mask.params, quad, width, height, &glyphs, scale)
            } else {
                rasterize_mask_stroke(&mask.kind, &mask.params, quad, width, height, scale)
            } {
                self.upload(&stroke_id, width, height, &pixels);
                self.strokes.insert(element_id.to_owned(), key);
                true
            } else {
                false
            };
            uploaded.then(|| LayerDescriptor {
                texture_id: stroke_id,
                transform: QuadTransformDescriptor {
                    center_x: width as f32 / 2.0,
                    center_y: height as f32 / 2.0,
                    width: width as f32,
                    height: height as f32,
                    rotation_degrees: 0.0,
                    flip_x: false,
                    flip_y: false,
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effect_pass_groups: Vec::new(),
                mask: None,
            })
        } else {
            None
        };
        (descriptor, stroke)
    }

    fn upload(&mut self, texture_id: &str, width: u32, height: u32, bgra: &[u8]) {
        let pixels: std::borrow::Cow<[u8]> = if self.context.pixels_are_bgra() {
            std::borrow::Cow::Borrowed(bgra)
        } else {
            let mut rgba = bgra.to_vec();
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            std::borrow::Cow::Owned(rgba)
        };
        if let Some(slot) = self.slots.get(texture_id)
            && slot.width == width
            && slot.height == height
        {
            self.context
                .write_texture(&slot.texture, width, height, &pixels);
            return;
        }
        let texture = self
            .context
            .upload_texture(width, height, &pixels, "render-layer-source");
        self.compositor
            .upsert_texture(texture_id.to_owned(), texture.clone());
        self.slots.insert(
            texture_id.to_owned(),
            TextureSlot {
                texture,
                width,
                height,
            },
        );
    }

    /// Stops decoder processes. The next render reopens what it needs.
    pub fn release_decoders(&mut self) {
        for source in self.videos.values_mut() {
            source.release();
        }
    }

    /// Forgets everything cached for a media item (after it was removed or
    /// its file changed).
    pub fn forget_media(&mut self, media_id: &str) {
        self.videos.remove(media_id);
        self.images.remove(media_id);
        let texture_id = format!("media:{media_id}");
        if self.slots.remove(&texture_id).is_some() {
            self.compositor.release_texture(&texture_id);
        }
    }

    /// Drops cached text rasters for elements that no longer exist.
    pub fn prune(&mut self, project: &Project) {
        let stale: Vec<String> = self
            .texts
            .keys()
            .filter(|id| project.locate(id).is_none())
            .cloned()
            .collect();
        for id in stale {
            self.texts.remove(&id);
            let texture_id = format!("text:{id}");
            self.slots.remove(&texture_id);
            self.compositor.release_texture(&texture_id);
        }
        let stale: Vec<String> = self
            .masks
            .keys()
            .filter(|id| {
                project
                    .element(id)
                    .is_none_or(|(_, element)| element.masks.is_empty())
            })
            .cloned()
            .collect();
        for id in stale {
            self.masks.remove(&id);
            let texture_id = format!("mask:{id}");
            self.slots.remove(&texture_id);
            self.compositor.release_texture(&texture_id);
        }
        let media_ids: Vec<String> = self.videos.keys().chain(self.images.keys()).cloned().collect();
        for id in media_ids {
            if project.media_item(&id).is_none() {
                self.forget_media(&id);
            }
        }
    }
}

fn media_item<'a>(project: &'a Project, media_id: &str) -> Result<&'a MediaItem, RenderError> {
    project
        .media_item(media_id)
        .ok_or_else(|| RenderError::MissingMedia(media_id.to_owned()))
}

/// How wide to decode a video so it is sharp at the size it is shown,
/// rounded up to 16 and never larger than the source.
fn decode_width(shown_width: f32, source_width: u32) -> u32 {
    let wanted = (shown_width.max(16.0) as u32).div_ceil(16) * 16;
    wanted.min(source_width.max(16)).max(16)
}

/// A transition's stored parameters as the compositor reads them.
fn transition_params(params: &Params) -> HashMap<String, TransitionParamDescriptor> {
    params
        .iter()
        .filter_map(|(key, value)| {
            let value = match value {
                ParamValue::Bool(value) => TransitionParamDescriptor::Bool(*value),
                ParamValue::Number(value) => TransitionParamDescriptor::Number(*value),
                ParamValue::Json(serde_json::Value::Array(items)) => TransitionParamDescriptor::Vector(
                    items.iter().filter_map(serde_json::Value::as_f64).collect(),
                ),
                _ => return None,
            };
            Some((key.clone(), value))
        })
        .collect()
}

fn parse_blend_mode(name: &str) -> BlendMode {
    match name {
        "darken" => BlendMode::Darken,
        "multiply" => BlendMode::Multiply,
        "color-burn" => BlendMode::ColorBurn,
        "lighten" => BlendMode::Lighten,
        "screen" => BlendMode::Screen,
        "plus-lighter" => BlendMode::PlusLighter,
        "color-dodge" => BlendMode::ColorDodge,
        "overlay" => BlendMode::Overlay,
        "soft-light" => BlendMode::SoftLight,
        "hard-light" => BlendMode::HardLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_width_rounds_and_clamps() {
        assert_eq!(decode_width(300.0, 1920), 304);
        assert_eq!(decode_width(5000.0, 1920), 1920);
        assert_eq!(decode_width(1.0, 1920), 16);
        assert_eq!(decode_width(100.0, 8), 16);
    }

    #[test]
    fn blend_modes_parse() {
        assert_eq!(parse_blend_mode("screen"), BlendMode::Screen);
        assert_eq!(parse_blend_mode("nonsense"), BlendMode::Normal);
    }
}

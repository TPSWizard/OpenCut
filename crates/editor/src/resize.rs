//! Resize project: a new canvas size with every clip, text and caption
//! re-fitted (the web app's `src/resize/plan.ts`, checked against the same
//! `fixtures/resize-project/cases.json`). The old picture is scaled about
//! the centre by the content factor (fit: the whole old frame shows; fill:
//! the new frame is covered, edges cropped; keep: the same pixel sizes), so
//! positions, keyframes and pixel settings follow it; each element's own
//! scale allows for how its base size follows the canvas (pictures fit the
//! canvas, text follows its height). Captions are placed again at the
//! bottom of the new canvas.

use crate::animation::Channel;
use crate::{CanvasSize, Element, ElementKind, Params, Project};

/// What happens to the content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeMode {
    /// The whole old picture shows.
    #[default]
    Fit,
    /// The new canvas is covered; the edges are cropped.
    Fill,
    /// Pixel sizes stay as they were.
    Keep,
}

impl ResizeMode {
    pub const ALL: [ResizeMode; 3] = [ResizeMode::Fit, ResizeMode::Fill, ResizeMode::Keep];

    pub fn id(self) -> &'static str {
        match self {
            ResizeMode::Fit => "fit",
            ResizeMode::Fill => "fill",
            ResizeMode::Keep => "keep",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }
}

type Size = (f64, f64);

/// How much the content grows.
pub fn content_factor(mode: ResizeMode, from: Size, to: Size) -> f64 {
    let (rw, rh) = (to.0 / from.0, to.1 / from.1);
    match mode {
        ResizeMode::Fit => rw.min(rh),
        ResizeMode::Fill => rw.max(rh),
        ResizeMode::Keep => 1.0,
    }
}

fn contain_scale(canvas: Size, source: Size) -> f64 {
    (canvas.0 / source.0).min(canvas.1 / source.1)
}

/// The scale factor of a video, picture, sticker or shape of `source` size.
pub fn picture_factor(mode: ResizeMode, from: Size, to: Size, source: Size) -> f64 {
    content_factor(mode, from, to) * contain_scale(from, source) / contain_scale(to, source)
}

/// The scale factor of a text element (its font size follows the canvas height).
pub fn text_factor(mode: ResizeMode, from: Size, to: Size) -> f64 {
    content_factor(mode, from, to) * from.1 / to.1
}

/// Captions' bottom margin as a share of the height: clear of the platforms' buttons on a tall canvas.
pub fn caption_margin(to: Size) -> f64 {
    if to.1 > to.0 { 0.25 } else { 0.05 }
}

/// Params in canvas pixels, scaled with the content.
pub const PIXEL_PARAMS: [&str; 4] = ["outline.width", "shadow.offsetX", "shadow.offsetY", "shadow.blur"];
const POSITION_PATHS: [&str; 2] = ["transform.positionX", "transform.positionY"];
const SCALE_PATHS: [&str; 2] = ["transform.scaleX", "transform.scaleY"];
/// The font size reference: a font size of 1 is 1/90 of the canvas height.
const FONT_SIZE_SCALE_REFERENCE: f64 = 90.0;

fn scale_channel(channel: &mut Channel, factor: f64) {
    for key in &mut channel.keys {
        key.value *= factor;
        if let Some(handle) = &mut key.left_handle {
            handle.dv *= factor;
        }
        if let Some(handle) = &mut key.right_handle {
            handle.dv *= factor;
        }
    }
}

fn scale_param(params: &mut Params, key: &str, factor: f64) {
    if let Some(value) = params.get(key).and_then(|value| value.as_number()) {
        params.set(key, value * factor);
    }
}

/// What a resize changed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResizeReport {
    pub content: f64,
    pub elements: usize,
    pub captions: usize,
}

struct Plan<'a> {
    mode: ResizeMode,
    from: Size,
    to: Size,
    k: f64,
    project_media: &'a [crate::MediaItem],
}

impl Plan<'_> {
    fn media_size(&self, media_id: &str) -> Option<Size> {
        let media = self.project_media.iter().find(|media| media.id == media_id)?;
        Some((media.width? as f64, media.height? as f64)).filter(|(w, h)| *w > 0.0 && *h > 0.0)
    }

    /// Re-fits one element (inside compounds too), counting it in `report`.
    fn element(&self, element: &mut Element, report: &mut ResizeReport) {
        let own = match &mut element.kind {
            ElementKind::Compound { tracks } => {
                for track in tracks {
                    for child in &mut track.elements {
                        self.element(child, report);
                    }
                }
                return;
            }
            ElementKind::Video { media_id } | ElementKind::Image { media_id } => match self.media_size(media_id) {
                Some(source) => picture_factor(self.mode, self.from, self.to, source),
                None => return,
            },
            ElementKind::Sticker { intrinsic_width, intrinsic_height, .. } => {
                let source = match (intrinsic_width, intrinsic_height) {
                    (Some(w), Some(h)) if *w > 0.0 && *h > 0.0 => (*w, *h),
                    _ => (1.0, 1.0),
                };
                picture_factor(self.mode, self.from, self.to, source)
            }
            ElementKind::Text if !element.words.is_empty() => {
                self.caption(element);
                report.elements += 1;
                report.captions += 1;
                return;
            }
            ElementKind::Text => text_factor(self.mode, self.from, self.to),
            // Drawings are in canvas units: they span the new canvas as they are.
            ElementKind::Audio { .. } | ElementKind::Graphic { .. } => return,
        };
        let factors = POSITION_PATHS
            .iter()
            .map(|path| (*path, self.k))
            .chain(SCALE_PATHS.iter().map(|path| (*path, own)))
            .chain(PIXEL_PARAMS.iter().map(|key| (*key, self.k)));
        for (path, factor) in factors {
            scale_param(&mut element.params, path, factor);
            if let Some(channel) = element.animations.get_mut(path) {
                scale_channel(channel, factor);
            }
        }
        for mask in &mut element.masks {
            scale_param(&mut mask.params, "feather", self.k);
            scale_param(&mut mask.params, "strokeWidth", self.k);
        }
        report.elements += 1;
    }

    /// A caption placed again: centred, its lines above the bottom margin.
    fn caption(&self, element: &mut Element) {
        let (_, height) = self.to;
        let font_px = element.params.number("fontSize", 15.0) * height / FONT_SIZE_SCALE_REFERENCE
            * element.params.number("transform.scaleY", 1.0).abs();
        let lines = element.params.text("content", "").lines().count().max(1) as f64;
        let block = lines * font_px * element.params.number("lineHeight", 1.2);
        let position_y = height / 2.0 - caption_margin(self.to) * height - block / 2.0;
        element.params.set("transform.positionX", 0.0).set("transform.positionY", position_y);
        for path in POSITION_PATHS {
            element.animations.remove(path);
        }
    }
}

/// Changes the canvas to `canvas` and re-fits every track's content.
pub fn resize_project(project: &mut Project, canvas: CanvasSize, mode: ResizeMode) -> ResizeReport {
    let from = (project.settings.canvas.width as f64, project.settings.canvas.height as f64);
    let to = (canvas.width as f64, canvas.height as f64);
    let plan = Plan { mode, from, to, k: content_factor(mode, from, to), project_media: &project.media };
    let mut report = ResizeReport { content: plan.k, ..Default::default() };
    let mut tracks = std::mem::take(&mut project.tracks);
    for track in &mut tracks {
        for element in &mut track.elements {
            plan.element(element, &mut report);
        }
    }
    project.tracks = tracks;
    project.settings.canvas = canvas;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works_out_the_factors_like_the_web() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/resize-project/cases.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 10);
        let pair = |value: &serde_json::Value| (value[0].as_f64().unwrap(), value[1].as_f64().unwrap());
        for case in cases {
            let (from, to, source) = (pair(&case["from"]), pair(&case["to"]), pair(&case["source"]));
            let mode = ResizeMode::from_id(case["mode"].as_str().unwrap()).unwrap();
            let close = |value: f64, key: &str| (value - case[key].as_f64().unwrap()).abs() < 1e-12;
            assert!(close(content_factor(mode, from, to), "content"), "{}", case["name"]);
            assert!(close(picture_factor(mode, from, to, source), "picture"), "{}", case["name"]);
            assert!(close(text_factor(mode, from, to), "text"), "{}", case["name"]);
            assert!(close(caption_margin(to), "captionMargin"), "{}", case["name"]);
        }
    }

    #[test]
    fn refits_clips_text_keyframes_and_captions() {
        use crate::{Command, Keyframe, MediaItem, MediaKind, Segment, TrackPlacement, apply};
        let mut project = Project::new("Resize");
        project.settings.canvas = CanvasSize { width: 1920, height: 1080 };
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "m".to_owned(),
                name: "m.mp4".to_owned(),
                kind: MediaKind::Video,
                path: "m.mp4".into(),
                duration: Some(time::MediaTime::from_seconds_f64(5.0).unwrap()),
                width: Some(1920),
                height: Some(1080),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let mut clip = Element::from_media(project.media_item("m").unwrap(), time::MediaTime::ZERO);
        clip.params.set("transform.positionX", 100.0).set("outline.width", 4.0);
        let mut channel = Channel::default();
        channel.keys.push(Keyframe::new(time::MediaTime::ZERO, 200.0, Segment::default()));
        clip.animations.insert("transform.positionX".to_owned(), channel);
        let mut mask = crate::Mask::rectangle();
        mask.params.set("feather", 10.0);
        clip.masks.push(mask);
        let mut title = Element::text("Title", time::MediaTime::ZERO);
        title.params.set("transform.positionY", 300.0);
        let mut caption = Element::text("hi there", time::MediaTime::ZERO);
        caption.words.push(crate::CaptionWord {
            text: "hi".to_owned(),
            start: time::MediaTime::ZERO,
            end: time::MediaTime::from_seconds_f64(0.5).unwrap(),
            keyword: false,
        });
        caption.params.set("fontSize", 5.0).set("transform.positionY", 400.0);
        let ids = [clip.id.clone(), title.id.clone(), caption.id.clone()];
        for element in [clip, title, caption] {
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let canvas = CanvasSize { width: 1080, height: 1920 };
        apply(&mut project, &Command::ResizeProject { canvas, mode: ResizeMode::Fill }).unwrap();
        assert_eq!(project.settings.canvas, canvas);
        let find = |id: &str| project.tracks.iter().flat_map(|track| &track.elements).find(|element| element.id == id).unwrap();
        let k = 1920.0 / 1080.0;
        let (from, to) = ((1920.0, 1080.0), (1080.0, 1920.0));
        let clip = find(&ids[0]);
        assert!((clip.params.number("transform.positionX", 0.0) - 100.0 * k).abs() < 1e-9);
        assert!((clip.params.number("outline.width", 0.0) - 4.0 * k).abs() < 1e-9);
        let scale = picture_factor(ResizeMode::Fill, from, to, from);
        assert!((clip.params.number("transform.scaleX", 0.0) - scale).abs() < 1e-9);
        // The landscape clip now covers the portrait canvas: drawn height = canvas height.
        assert!((1080.0 * contain_scale(to, from) * scale - 1920.0).abs() < 1e-6);
        assert!((clip.animations["transform.positionX"].keys[0].value - 200.0 * k).abs() < 1e-9);
        assert!((clip.masks[0].params.number("feather", 0.0) - 10.0 * k).abs() < 1e-9);
        let title = find(&ids[1]);
        assert!((title.params.number("transform.positionY", 0.0) - 300.0 * k).abs() < 1e-9);
        assert!((title.params.number("transform.scaleX", 1.0) - text_factor(ResizeMode::Fill, from, to)).abs() < 1e-9);
        let caption = find(&ids[2]);
        let font_px = 5.0 * 1920.0 / 90.0;
        let expected = 960.0 - 0.25 * 1920.0 - font_px * 1.2 / 2.0;
        assert!((caption.params.number("transform.positionY", 0.0) - expected).abs() < 1e-9);
        assert_eq!(caption.params.number("transform.positionX", 1.0), 0.0);
    }
}

//! Rounded corners, an outline and a drop shadow (or glow) for a visual
//! clip, from its params: `cornerRadius` (percent of half the shorter side;
//! not on text, whose box has its own), `outline.width` / `outline.color`,
//! and `shadow.enabled` / `shadow.color` / `shadow.opacity` /
//! `shadow.blur` / `shadow.offsetX` / `shadow.offsetY`. Lengths are canvas
//! pixels. The compositor draws them (`LayerStyleDescriptor`); the web
//! editor's `@/layer-style/style` reads the same params.

use crate::{Params, frame::parse_hex_color};

pub const CORNER_RADIUS_MAX: f64 = 100.0;
pub const OUTLINE_WIDTH_MAX: f64 = 50.0;
pub const SHADOW_BLUR_MAX: f64 = 100.0;
pub const SHADOW_OFFSET_MAX: f64 = 500.0;
pub const SHADOW_OPACITY_DEFAULT: f64 = 0.6;
pub const SHADOW_BLUR_DEFAULT: f64 = 16.0;
pub const SHADOW_OFFSET_DEFAULT: f64 = 8.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerStyle {
    /// Percent (0–100) of half the clip's shorter side.
    pub corner_radius: f32,
    /// Width in canvas pixels and straight-alpha colour.
    pub outline: Option<(f32, [f32; 4])>,
    pub shadow: Option<LayerShadow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerShadow {
    /// Straight-alpha colour with the opacity folded into alpha.
    pub color: [f32; 4],
    /// Canvas pixels.
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

impl LayerStyle {
    pub fn is_empty(&self) -> bool {
        self.corner_radius <= 0.0 && self.outline.is_none() && self.shadow.is_none()
    }

    /// The style from an element's params; `is_text` leaves out the corners.
    pub fn from_params(params: &Params, is_text: bool) -> Self {
        let number = |key: &str, fallback: f64, min: f64, max: f64| {
            let value = params.number(key, fallback);
            if value.is_finite() { value.clamp(min, max) } else { fallback }
        };
        let color = |key: &str, fallback: [f32; 4]| parse_hex_color(&params.text(key, "")).unwrap_or(fallback);
        let corner_radius = if is_text { 0.0 } else { number("cornerRadius", 0.0, 0.0, CORNER_RADIUS_MAX) as f32 };
        let outline_width = number("outline.width", 0.0, 0.0, OUTLINE_WIDTH_MAX) as f32;
        let outline = (outline_width > 0.0).then(|| (outline_width, color("outline.color", [1.0; 4])));
        let shadow = params.bool("shadow.enabled", false).then(|| {
            let mut shadow_color = color("shadow.color", [0.0, 0.0, 0.0, 1.0]);
            shadow_color[3] *= number("shadow.opacity", SHADOW_OPACITY_DEFAULT, 0.0, 1.0) as f32;
            LayerShadow {
                color: shadow_color,
                offset_x: number("shadow.offsetX", SHADOW_OFFSET_DEFAULT, -SHADOW_OFFSET_MAX, SHADOW_OFFSET_MAX) as f32,
                offset_y: number("shadow.offsetY", SHADOW_OFFSET_DEFAULT, -SHADOW_OFFSET_MAX, SHADOW_OFFSET_MAX) as f32,
                blur: number("shadow.blur", SHADOW_BLUR_DEFAULT, 0.0, SHADOW_BLUR_MAX) as f32,
            }
        });
        Self { corner_radius, outline, shadow: shadow.filter(|shadow| shadow.color[3] > 0.0) }
    }

    /// The corner radius in pixels for a quad `width` by `height` pixels.
    pub fn corner_radius_pixels(&self, width: f32, height: f32) -> f32 {
        (self.corner_radius / 100.0) * width.abs().min(height.abs()) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_style_params() {
        assert!(LayerStyle::from_params(&Params::new(), false).is_empty());
        let mut params = Params::new();
        params
            .set("cornerRadius", 50.0)
            .set("outline.width", 80.0)
            .set("outline.color", "#ff0000")
            .set("shadow.enabled", true)
            .set("shadow.opacity", 0.5)
            .set("shadow.offsetY", -4.0);
        let style = LayerStyle::from_params(&params, false);
        assert_eq!(style.corner_radius, 50.0);
        assert_eq!(style.corner_radius_pixels(400.0, 200.0), 50.0);
        assert_eq!(style.outline, Some((50.0, [1.0, 0.0, 0.0, 1.0])), "width capped at 50");
        let shadow = style.shadow.unwrap();
        assert_eq!(shadow.color, [0.0, 0.0, 0.0, 0.5]);
        assert_eq!((shadow.offset_x, shadow.offset_y, shadow.blur), (8.0, -4.0, 16.0));
        assert_eq!(LayerStyle::from_params(&params, true).corner_radius, 0.0, "no corners on text");
    }
}

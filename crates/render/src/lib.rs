//! Pixels and samples for native front ends. [`Renderer`] turns an
//! [`editor::Project`] at a point in time into a BGRA frame: it decodes
//! media through the `media` crate, rasterises text with cosmic-text, and
//! composites everything with the shared `compositor` crate on the GPU.
//! [`Mixer`] produces the matching audio for playback.

mod effects;
mod error;
mod mask;
mod mixer;
mod renderer;
mod text;

pub use effects::{backdrop_blur_passes, effect_pass_groups};
pub use error::RenderError;
pub use mask::{rasterize_mask, rasterize_mask_stroke, rasterize_text_mask, rasterize_text_mask_stroke};
pub use mixer::Mixer;
pub use renderer::{RenderedFrame, Renderer, RendererOptions};
pub use text::{TextRaster, TextRasterizer};

//! Pixels and samples for native front ends. [`Renderer`] turns an
//! [`editor::Project`] at a point in time into a BGRA frame: it decodes
//! media through the `media` crate, rasterises text with cosmic-text, and
//! composites everything with the shared `compositor` crate on the GPU.
//! [`Mixer`] produces the matching audio for playback.

mod cover;
mod effects;
mod export;
mod export_kinds;
mod retime_audio;
mod cache;
mod error;
mod mask;
mod mixer;
mod renderer;
mod text;

pub use cover::{embed_cover, render_cover, save_cover_png, suggest_cover_time};
pub use effects::{backdrop_blur_passes, effect_pass_groups};
pub use export_kinds::{GifJob, export_audio, export_gif, export_png, export_png_sequence, gif_size, sequence_file_name};
pub use export::{
    ExportJob, MixLoudness, export_project, export_project_with_report, gain_keys, still_times, transparent_project,
};
pub use cache::{clear_regenerable_cache, is_regenerable, regenerable_cache};
pub use retime_audio::{default_cache_dir, needs_render as audio_needs_render, rendered_clip};
pub use error::RenderError;
pub use mask::{rasterize_mask, rasterize_mask_stroke, rasterize_text_mask, rasterize_text_mask_stroke};
pub use mixer::{Mixer, TrackMeters};
pub use renderer::{sticker_file_name, RenderedFrame, Renderer, RendererOptions};
pub use text::{TextRaster, TextRasterizer};

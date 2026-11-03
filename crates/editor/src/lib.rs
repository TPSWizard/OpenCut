//! The editor core: a project document, the commands that edit it, an undo
//! history, and planning of what is on screen (and audible) at a point in
//! time. Nothing here touches a GPU, a file decoder or a window; the
//! desktop app, the renderer and (through wasm, later) the web app build
//! on it.
//!
//! Times are [`time::MediaTime`] ticks. Names of element parameters match
//! the web Editor API (`transform.positionX`, `opacity`, `content`, ...)
//! so a project means the same thing on every front end.

mod animation;
mod audio;
mod commands;
mod document;
mod editor;
mod error;
mod frame;
mod params;
mod store;

pub use animation::{
    Animations, Channel, CurveHandle, KEY_SNAP, Keyframe, Segment, is_param_path,
    resolve_object_params, resolve_params, split_animations,
};
pub use audio::{AudioClipPlan, plan_audio};
pub use commands::{
    Command, Edge, ElementPatch, ElementRef, Outcome, SettingsPatch, TrackPlacement, apply,
    nearest_free_start,
};
pub use document::{
    Background, CanvasSize, Effect, Element, ElementKind, Lane, Mask, MaskSize, MediaItem, MediaKind,
    PROJECT_FORMAT_VERSION,
    Project, ProjectSettings, Track, TrackKind, now_millis,
};
pub use editor::Editor;
pub use error::EditorError;
pub use frame::{
    BackdropPlan, EffectPlan, FONT_SIZE_RATIO_REFERENCE, FONT_SIZE_REFERENCE, FramePlan, LayerPlan, LayerSource, LayerTransform, MaskPlan, TextAlign,
    TextBackground, TextDecoration, TextStyle, fit_contain, parse_hex_color, plan_frame,
};
pub use params::{ParamValue, Params, audio_defaults, text_defaults, visual_defaults};
pub use store::{ProjectStore, ProjectSummary};
pub use time;

/// Generates a fresh identifier for projects, media, tracks and elements.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

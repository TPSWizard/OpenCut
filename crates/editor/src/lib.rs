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
mod bundle;
mod captions;
mod commands;
mod document;
mod ducking;
mod edits;
mod editor;
mod effect_catalogue;
mod error;
mod frame;
mod freeform;
mod loudness;
mod params;
mod platforms;
mod silence;
mod store;
mod subtitles;
mod transition;

pub use animation::{
    Animations, Channel, CurveHandle, KEY_SNAP, Keyframe, Segment, format_linear_rgba, is_param_path,
    parse_color_to_linear_rgba, resolve_object_params, resolve_params, split_animations,
};
pub use audio::{AudioClipPlan, plan_audio};
pub use bundle::{BUNDLE_FORMAT, BUNDLE_FORMAT_VERSION, ImportReport, WEB_PROJECT_VERSION, export_bundle, import_bundle};
pub use captions::{
    CaptionChunk, DEFAULT_WORDS_PER_CAPTION, GENERATED_CAPTION_HIGHLIGHT, MAX_CAPTION_WORD_GAP_SECONDS,
    MIN_CAPTION_DURATION_SECONDS, TimedWord, build_caption_chunks, caption_element,
};
pub use commands::{
    Command, Edge, ElementPatch, ElementRef, Outcome, SettingsPatch, TrackPlacement, apply,
    nearest_free_start,
};
pub use document::{
    Background, CanvasSize, CaptionWord, Effect, Element, ElementKind, Lane, Mask, MaskSize, MediaItem, MediaKind,
    PROJECT_FORMAT_VERSION,
    Project, ProjectSettings, Track, TrackKind, Transition, now_millis,
};
pub use freeform::{
    ElementQuad, FreeformPoint, canvas_to_freeform_local, closest_on_freeform_segment, cubic_point, element_quad,
    freeform_local_to_canvas, freeform_segment_count, insert_freeform_point, read_freeform_path,
    recenter_freeform_path, remove_freeform_points, write_freeform_path,
};
pub use ducking::{
    DuckingKey, DuckingOptions, Interval, SILENCE_DB, build_ducking_keys, mean_square_to_db, rms_db_windows,
    speech_intervals, total_seconds,
};
pub use editor::Editor;
pub use edits::{
    RipplePlan, collect_edit_points, delete_range_plan, elements_crossing, merge_spans, next_edit_point,
    previous_edit_point, ripple_delete_plan,
};
pub use effect_catalogue::{EFFECT_CATALOGUE, EffectParamSpec, EffectSpec, effect_spec};
pub use error::EditorError;
pub use frame::{
    BackdropPlan, EffectPlan, FONT_SIZE_RATIO_REFERENCE, FONT_SIZE_REFERENCE, FrameItemPlan, FramePlan, HighlightMode, LayerPlan, LayerSource, LayerTransform, MaskPlan, TextAlign,
    TextBackground, TextDecoration, TextHighlight, TextStyle, TransitionPlan, active_word, caption_highlight, fit_contain, parse_hex_color, plan_frame,
};
pub use transition::{
    ADJACENCY_TOLERANCE, TransitionWindow, is_adjacent, next_adjacent, transition_window, window_into, window_of,
};
pub use params::{ParamValue, Params, audio_defaults, text_defaults, visual_defaults};
pub use platforms::{
    PLATFORM_AUDIO_BITRATE, PLATFORM_PRESETS, PlatformEncodeSettings, PlatformFix, PlatformIssue, PlatformIssueKind,
    PlatformPreset, PlatformSeverity, SafeArea, Verified, check_platform_export, describe_aspect,
    estimate_platform_file_bytes, format_bytes, format_duration, platform_encode_settings, platform_preset,
    recommended_video_bitrate,
};
pub use silence::{RippleScope, SilenceOptions, silence_intervals};
pub use loudness::{
    ABSOLUTE_GATE_LUFS, LOUDNESS_TARGETS, LoudnessReport, LoudnessTarget, deinterleave, gated_loudness, loudness_range,
    measure_loudness, normalization_gain, oversampling_phases, true_peak,
};
pub use store::{ProjectStore, ProjectSummary};
pub use subtitles::{
    ExportCue, ExportCueStyle, ExportWord, SubtitleFormat, ass_colour, ass_placement, collect_caption_cues,
    export_subtitles, subtitle_file_name, write_ass, write_srt, write_subtitles, write_vtt,
};
pub use time;

/// Generates a fresh identifier for projects, media, tracks and elements.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

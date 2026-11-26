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
mod auto_zoom;
mod assets;
mod audio;
mod audio_crossfade;
pub use audio_crossfade::{
    CROSSFADE_CURVE_PARAM, CROSSFADE_MAX_SECONDS, CROSSFADE_PARAM, CrossfadeCurve, CrossfadeOutcome, clip_crossfade, crossfade_commands,
    crossfaded_elements, fade_curve, remove_crossfade_command, track_clips,
};
mod beats;
mod clip_animation;
mod broll;
mod bundle;
mod captions;
mod caption_keywords;
mod colour_match;
mod credit_roll;
mod text_templates;
mod colour_tools;
mod drawing;
mod tracked;
mod trim_tools;
mod caption_edit;
mod subtitle_import;
mod corner_pin;
mod motion_blur;
mod deflicker;
mod multicam_group;
mod face_retouch;
pub use face_retouch::{face_blur_commands, face_retouch_commands};
pub use facetrack::{FACE_TRACK_PARAM, FaceBox, FaceTrack};
pub use multicam_group::{
    MULTICAM_ANGLE, MULTICAM_CUTS, MULTICAM_GROUP, MulticamAngle, multicam_clear_commands, multicam_cuts, multicam_group,
    multicam_switch_commands, multicam_sync_commands,
};

/// Rich text: the param holding a text clip's styled ranges (`richtext`).
pub const RICH_TEXT_PARAM: &str = "richText";
pub use richtext::{CharStyle as RichCharStyle, Patch as RichPatch, RichText};
mod commands;
mod cover;
mod compound;
mod document;
mod ducking;
mod edits;
mod editor;
mod effect_catalogue;
mod error;
mod frame;
mod fillers;
mod freeform;
mod freeze;
mod layer_style;
mod luts;
mod markers;
mod open_link;
mod params;
mod platforms;
mod relink;
mod resize;
mod retime;
mod picture;
mod reframe;
mod voiceover;
mod shots;
mod stock;
mod silence;
mod store;
mod subtitles;
mod split_screen;
mod transcript;
mod transition;

pub use animation::{
    Animations, Channel, CurveHandle, KEY_SNAP, Keyframe, Segment, format_linear_rgba, is_param_path,
    parse_color_to_linear_rgba, resolve_object_params, resolve_params, split_animations,
};
pub use audio::{AUDIO_FADE_MAX_SECONDS, AudioClipPlan, clip_audio_fx, clip_fades, clip_pan, pan_stereo, plan_audio, track_settings};
pub use trackmix::{TRACK_VOLUME_MAX_DB, TRACK_VOLUME_MIN_DB, TrackMix, TrackSettings, pan_clip_and_track, track_mix, volume_gain};
pub use beats::{BeatAnalysis, BeatOptions, onset_strength, track_beats};
pub use broll::{MIN_SHOT_SECONDS, Shot, WORDS_PER_SECOND, plan_shots, query_for, shot_seconds};
pub use open_link::bundle_url_from_link;
pub use assets::{AssetEntry, AssetLookups, BrandInfo, IconSetInfo, MediaOrigin, ASSET_MANIFEST_ENTRY, ASSET_MANIFEST_FORMAT, SYSTEM_FONTS, asset_manifest, fontsource_slug, license_requires_credit, manifest_credits_text};
pub use bundle::{BUNDLE_FORMAT, BUNDLE_FORMAT_VERSION, ImportReport, WEB_PROJECT_VERSION, export_bundle, export_bundle_with, import_bundle, import_bundle_as_new, project_asset_manifest};
pub use captions::{
    CaptionChunk, DEFAULT_WORDS_PER_CAPTION, GENERATED_CAPTION_HIGHLIGHT, MAX_CAPTION_WORD_GAP_SECONDS,
    MIN_CAPTION_DURATION_SECONDS, TimedWord, build_caption_chunks, caption_element,
};
pub use commands::{
    Command, Edge, ElementPatch, ElementRef, Outcome, SettingsPatch, TrackPlacement, apply,
    nearest_free_start,
};
pub use document::{
    CUTOUT_BACKGROUND_BLUR_DEFAULT, Cutout, DRAWING_DEFINITION, StockCredit,
    Background, CanvasSize, CaptionWord, Effect, Element, ElementKind, Lane, Mask, MaskSize, MediaItem, MediaKind,
    PROJECT_FORMAT_VERSION,
    Project, ProjectSettings, Track, TrackKind, Transition, now_millis,
};
pub use fillers::{
    FILLER_LANGUAGES, FILLER_PROMPT, FILLER_WORDS, FillerHit, FillerOptions, filler_intervals, filler_set, find_fillers,
    normalize_word,
};
pub use clip_animation::{
    ANIMATION_DURATION_DEFAULT, ANIMATION_DURATION_MAX, ANIMATION_DURATION_MIN, ANIMATION_IN_KINDS,
    ANIMATION_LOOP_KINDS, ANIMATION_LOOP_PERIOD_DEFAULT, ANIMATION_LOOP_PERIOD_MAX, ANIMATION_LOOP_PERIOD_MIN,
    ANIMATION_OUT_KINDS, ClipAnimationState, ClipAnimations, RevealMode, TEXT_ONLY_ANIMATIONS, animation_label,
    animation_windows, clip_animation_state_at, read_clip_animations, revealed_char_count,
};
pub use layer_style::{
    CORNER_RADIUS_MAX, LayerShadow, LayerStyle, OUTLINE_WIDTH_MAX, SHADOW_BLUR_DEFAULT, SHADOW_BLUR_MAX,
    SHADOW_OFFSET_DEFAULT, SHADOW_OFFSET_MAX, SHADOW_OPACITY_DEFAULT,
};
pub use freeze::{FREEZE_DEFAULT_SECONDS, FREEZE_MAX_SECONDS, FREEZE_MIN_SECONDS, freeze_still};
pub use retakes::{Retake, RetakeKind, RetakeOptions, retake_words};
pub use transcript::{TRANSCRIPT_EDGE_PADDING, retime_captions, transcript_cuts, transcript_retakes};
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
pub use colour_tools::{
    COLOUR_WHEEL_ANGLES, COLOUR_WHEEL_RANGE, COLOUR_WHEELS, HSL_BANDS, HSL_CONTROLS, HSL_HUE_RANGE, colour_wheel_move,
    colour_wheel_offsets, colour_wheel_point, colour_wheel_rgba, hsl_band_swatch, hsl_band_value, hsl_param_key,
};
pub use subtitle_import::{read_subtitle_file, subtitle_import_command};
pub use caption_edit::{CaptionEdit, caption_edit_command, caption_match_count};
pub use trim_tools::{TrimTool, trim_tool_command};
pub use tracked::{tracked_corner_pin_command, tracked_mask_command};
pub use drawing::{add_stroke_command, is_drawing, remove_last_stroke_command};
pub use effect_catalogue::{EFFECT_CATALOGUE, EffectParamSpec, EffectSpec, LIGHT_LEAK_COLOURS, effect_spec};
pub use error::EditorError;
pub use frame::{
    BackdropPlan, EffectPlan, background_blur_layer, FONT_SIZE_RATIO_REFERENCE, FONT_SIZE_REFERENCE, FrameItemPlan, FramePlan, HighlightMode, LayerPlan, LayerSource, LayerTransform, MaskPlan, CutoutPlan, TextAlign,
    TextBackground, TextDecoration, TextHighlight, TextKeywords, LayerZoom, TextStyle, TransitionPlan, active_word, caption_highlight, caption_keywords, fit_contain,
    parse_hex_color, plan_frame,
};
pub use auto_zoom::{
    ClipZoom, MIN_PUNCH_SECONDS, SENTENCE_GAP_SECONDS, TAIL_SECONDS, ZOOM_FOCUS_Y_DEFAULT, ZOOM_RAMP_DEFAULT, ZOOM_RAMP_MAX, ZOOM_SCALE_MAX,
    ZOOM_STRENGTHS, ZoomPunch, ZoomWord, clip_words, format_punches, parse_punches, plan_punches, read_clip_zoom, zoom_at, zoom_offset,
};
pub use credit_roll::{
    CreditRoll, ROLL_SPEED_MAX, ROLL_SPEED_MIN, RollBox, RollDirection, credit_roll_position, credit_roll_seconds,
    credit_roll_span, read_credit_roll,
};
pub use text_templates::{
    PlacedTextItem, TextTemplate, TextTemplateCategory, TextTemplateItem, place_text_template, text_template,
    text_template_categories, text_template_elements, text_template_font_scale, text_templates,
};
pub use colour_match::{
    COLOUR_MATCH_LUT_SIZE, COLOUR_MATCH_NAME, ColourAccumulator, ColourStats, colour_match_commands, colour_match_lut, colour_match_param,
    is_colour_match_param, lab_to_rgb, rgb_to_lab, transfer_colour,
};
pub use cover::{
    COVER_SAMPLES, CoverBackground, CoverSettings, CoverShape, CoverScore, CoverStyle, CoverTitle, MAX_TITLE_LINES, cover_project,
    cover_sample_times, cover_size, cover_title, frame_score_bgra, frame_score_rgba, pick_cover_time, wrap_title,
};
pub use deflicker::{DEFLICKER_LEVELS, DEFLICKER_STRENGTH, DeflickerStrength, deflicker_gain, read_deflicker_levels, read_deflicker_strength};
pub use motion_blur::{MOTION_BLUR_MAX, MOTION_BLUR_PARAM, motion_blur_half_shutter, read_motion_blur};
pub use corner_pin::{
    CORNER_PIN_CORNERS, CORNER_PIN_ENABLED, CORNER_PIN_MAX, CORNER_PIN_MIN, PinBox, box_to_canvas, canvas_to_box,
    corner_pin_corners, corner_pin_key, corner_pin_keys, corner_pin_params, is_convex as is_corner_pin_drawable, read_corner_pin,
};
pub use resize::{PIXEL_PARAMS, ResizeMode, ResizeReport, caption_margin, content_factor, picture_factor, resize_project, text_factor};
pub use split_screen::{SPLIT_LAYOUTS, SplitPlacement, place_in_cell, plan_split_screen, split_layout_cells, split_screen_commands};
pub use caption_keywords::{
    KEYWORD_COLOR_DEFAULT, KEYWORD_SCALE_DEFAULT, KEYWORD_SCALE_MAX, STOP_WORDS, keyword_budget, keyword_core, keyword_score, pick_keyword_indices,
};
pub use transition::{
    ADJACENCY_TOLERANCE, TransitionWindow, in_window, is_adjacent, next_adjacent, previous_adjacent, transition_window,
    window_into, window_of,
};
pub use params::{ParamValue, Params, audio_defaults, text_defaults, visual_defaults};
pub use compound::{CompoundTrack, clip_element, compound_child_count, compound_children, expand_compounds, group_members, has_compounds};
pub use markers::{DEFAULT_MARKER_COLOR, Marker, chapters_text, elements_in_range, normalize_markers};
pub use relink::{RELINK_SEARCH_DEPTH, find_relink_candidates, missing_media};
pub use retime::{MAX_RETIME_RATE, MIN_RETIME_RATE, RampKey, Retime, clamp_retime_rate};
pub use platforms::{
    PLATFORM_AUDIO_BITRATE, PLATFORM_PRESETS, PlatformEncodeSettings, PlatformFix, PlatformIssue, PlatformIssueKind,
    PlatformPreset, PlatformSeverity, SafeArea, Verified, check_platform_export, describe_aspect,
    estimate_platform_file_bytes, format_bytes, format_duration, platform_encode_settings, platform_preset,
    recommended_video_bitrate,
};
pub use voiceover::{MAX_LINE_CHARS, PARAGRAPH_PAUSE, SENTENCE_PAUSE, ScriptLine, layout_lines, split_script};
pub use picture::{
    Picture, PictureCleanup, TracedPath, clean_picture, crop_to_content, decontaminate_white, feather_alpha, ink_tint,
    remove_flat_white, trace_main_path,
};
pub use reframe::{
    Detection, PathPoint, ReframeCamera, SUBJECT_FRAME_WIDTH, ReframeGeometry, ReframePlan, ReframeSample, Subject, axis_keys,
    clip_seconds_for_source, cover_scale, plan_path, plan_reframe, shot_subjects, subject_from_detections,
    visible_fraction,
};
pub use stock::{DEFAULT_API_URL, OPENVERSE_LICENSES, StockItem, credits_text, openverse_license_label, parse_openverse};
pub use shots::{
    FrameScore, HISTOGRAM_CUT_THRESHOLD, ShotOptions, colour_histogram, histogram_distance, pick_cuts, source_to_timeline,
};
pub use silence::{RippleScope, SilenceOptions, silence_intervals};
pub use loudness::{
    ABSOLUTE_GATE_LUFS, LOUDNESS_TARGETS, LoudnessReport, LoudnessTarget, MasterReport, Meter as LoudnessMeter, MeterReadings, deinterleave,
    gated_loudness, limit as limit_true_peak, loudness_range, master as master_mix, measure_loudness, normalization_gain, oversampling_phases,
    true_peak,
};
pub use luts::{
    LutTable, MAX_LUT_SIZE, base64_decode, base64_encode, decode_lut_param, encode_lut_param, from_hald, lut_key, parse_3dl,
    parse_cube,
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

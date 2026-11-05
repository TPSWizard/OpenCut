//! Platform export presets: the canvas, frame rate and limits each platform
//! publishes (or, where it publishes nothing, the figures third-party guides
//! agree on), the encoder settings YouTube documents for H.264 uploads, and
//! the part of a vertical frame the platform's own controls cover.
//!
//! The table matches `apps/web/src/platforms/catalogue.ts` figure for
//! figure; a change on one side goes on the other. `Verified::Official`
//! means the platform's help pages (CONFIRMED); `ThirdParty` means guides
//! and tools that may lag or disagree (INFERRED). Safe areas are approximate
//! everywhere: TikTok says the covered area depends on the caption length,
//! and every platform redraws its overlay now and then.

use time::FrameRate;

use crate::document::CanvasSize;

/// Margins of the frame the platform's own interface covers, as fractions
/// of the canvas width (left, right) and height (top, bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafeArea {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verified {
    Official,
    ThirdParty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformPreset {
    pub id: &'static str,
    pub label: &'static str,
    /// Canvas size the platform recommends.
    pub canvas: CanvasSize,
    /// Frame rate the platform recommends.
    pub fps: FrameRate,
    /// Longest upload in seconds, or none when nothing is published.
    pub max_duration_seconds: Option<f64>,
    /// Largest upload in bytes, or none when nothing is published.
    pub max_file_bytes: Option<u64>,
    /// Part of the frame the platform's controls leave visible, if any.
    pub safe_area: Option<SafeArea>,
    pub verified: Verified,
    /// One line on where the figures come from.
    pub source: &'static str,
}

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

const PORTRAIT: CanvasSize = CanvasSize { width: 1080, height: 1920 };

pub const PLATFORM_PRESETS: &[PlatformPreset] = &[
    PlatformPreset {
        id: "youtube",
        label: "YouTube",
        canvas: CanvasSize::HD_1080,
        fps: FrameRate::FPS_30,
        max_duration_seconds: None,
        max_file_bytes: None,
        safe_area: None,
        verified: Verified::Official,
        source: "YouTube recommended upload encoding settings (support.google.com/youtube/answer/1722171)",
    },
    PlatformPreset {
        id: "youtube-shorts",
        label: "YouTube Shorts",
        canvas: PORTRAIT,
        fps: FrameRate::FPS_30,
        max_duration_seconds: Some(180.0),
        max_file_bytes: None,
        // Third-party safe-zone tools converge on 288 px top, 672 px
        // bottom, 48 px left and 192 px right of a 1080×1920 frame.
        safe_area: Some(SafeArea { top: 0.15, bottom: 0.35, left: 0.045, right: 0.18 }),
        verified: Verified::ThirdParty,
        source: "Shorts length from YouTube Help; safe area from third-party safe-zone tools (approximate)",
    },
    PlatformPreset {
        id: "tiktok",
        label: "TikTok",
        canvas: PORTRAIT,
        fps: FrameRate::FPS_30,
        max_duration_seconds: Some(600.0),
        max_file_bytes: None,
        // Guides give 10 % top and 15 % bottom, with the icon column about
        // 150 px wide on the right. TikTok says the covered area depends
        // on the caption length.
        safe_area: Some(SafeArea { top: 0.1, bottom: 0.15, left: 0.04, right: 0.14 }),
        verified: Verified::ThirdParty,
        source: "Third-party TikTok spec guides (approximate); TikTok publishes no bitrate or fixed safe zone",
    },
    PlatformPreset {
        id: "instagram-reels",
        label: "Instagram Reels",
        canvas: PORTRAIT,
        fps: FrameRate::FPS_30,
        max_duration_seconds: Some(180.0),
        max_file_bytes: Some(4 * GB),
        // Guides keep about 250 px clear at the top and 350 px at the
        // bottom of a 1080×1920 frame, plus the icon column on the right.
        safe_area: Some(SafeArea { top: 0.13, bottom: 0.18, left: 0.04, right: 0.14 }),
        verified: Verified::ThirdParty,
        source: "Meta help pages for size and length; safe area from third-party guides (approximate)",
    },
    PlatformPreset {
        id: "linkedin",
        label: "LinkedIn",
        canvas: CanvasSize::HD_1080,
        fps: FrameRate::FPS_30,
        max_duration_seconds: Some(900.0),
        max_file_bytes: Some(5 * GB),
        safe_area: None,
        verified: Verified::Official,
        source: "LinkedIn Help: native video up to 5 GB and 15 minutes from the web",
    },
    PlatformPreset {
        id: "x",
        label: "X",
        canvas: CanvasSize { width: 1280, height: 720 },
        fps: FrameRate::FPS_30,
        max_duration_seconds: Some(140.0),
        max_file_bytes: Some(512 * MB),
        safe_area: None,
        verified: Verified::ThirdParty,
        source: "Third-party guides for standard accounts; X's own spec pages could not be reached",
    },
];

pub fn platform_preset(id: &str) -> Option<&'static PlatformPreset> {
    PLATFORM_PRESETS.iter().find(|preset| preset.id == id)
}

/// YouTube's recommended stereo AAC-LC bitrate, in bits per second.
pub const PLATFORM_AUDIO_BITRATE: u64 = 384_000;

/// Encoder settings for a platform upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformEncodeSettings {
    /// H.264 target bitrate in bits per second, from YouTube's SDR table.
    pub video_bitrate: u64,
    /// AAC stereo bitrate in bits per second.
    pub audio_bitrate: u64,
    /// Frames between key frames: a closed GOP of half the frame rate.
    pub gop_frames: u32,
}

/// YouTube's recommended SDR upload bitrate (bits per second) for a frame
/// size and rate. The tier follows the frame's shorter side, so portrait
/// 1080×1920 is 1080p. Rates above 40 fps take the high-rate column.
pub fn recommended_video_bitrate(canvas: CanvasSize, fps: f64) -> u64 {
    let side = canvas.width.min(canvas.height);
    let high = fps > 40.0;
    let megabits: f64 = match side {
        s if s >= 4320 => if high { 180.0 } else { 120.0 },
        s if s >= 2160 => if high { 60.0 } else { 40.0 },
        s if s >= 1440 => if high { 24.0 } else { 16.0 },
        s if s >= 1080 => if high { 12.0 } else { 8.0 },
        s if s >= 720 => if high { 7.5 } else { 5.0 },
        s if s >= 480 => if high { 4.0 } else { 2.5 },
        _ => if high { 1.5 } else { 1.0 },
    };
    (megabits * 1_000_000.0).round() as u64
}

pub fn platform_encode_settings(canvas: CanvasSize, fps: f64) -> PlatformEncodeSettings {
    PlatformEncodeSettings {
        video_bitrate: recommended_video_bitrate(canvas, fps),
        audio_bitrate: PLATFORM_AUDIO_BITRATE,
        gop_frames: ((fps / 2.0).round() as u32).max(1),
    }
}

/// Rough size of the file the platform settings produce.
pub fn estimate_platform_file_bytes(canvas: CanvasSize, fps: f64, duration_seconds: f64, include_audio: bool) -> u64 {
    let bits = recommended_video_bitrate(canvas, fps) + if include_audio { PLATFORM_AUDIO_BITRATE } else { 0 };
    ((bits as f64 * duration_seconds.max(0.0)) / 8.0).round() as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformIssueKind {
    Canvas,
    Fps,
    Duration,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformSeverity {
    /// A mismatch the export still works with.
    Warning,
    /// The platform would refuse the upload.
    Error,
}

/// Project setting that would clear an issue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlatformFix {
    Canvas(CanvasSize),
    Fps(FrameRate),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformIssue {
    pub kind: PlatformIssueKind,
    pub severity: PlatformSeverity,
    pub message: String,
    pub fix: Option<PlatformFix>,
}

pub fn describe_aspect(canvas: CanvasSize) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a.max(1) } else { gcd(b, a % b) }
    }
    let divisor = gcd(canvas.width, canvas.height);
    format!("{}:{}", canvas.width / divisor, canvas.height / divisor)
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= GB {
        let gb = bytes as f64 / GB as f64;
        if bytes >= 10 * GB { format!("{gb:.0} GB") } else { format!("{gb:.1} GB") }
    } else if bytes >= MB {
        format!("{} MB", (bytes as f64 / MB as f64).round() as u64)
    } else {
        format!("{} KB", ((bytes as f64 / 1024.0).round() as u64).max(1))
    }
}

pub fn format_duration(seconds: f64) -> String {
    let whole = seconds.round().max(0.0) as u64;
    let (minutes, rest) = (whole / 60, whole % 60);
    if minutes == 0 {
        format!("{rest} s")
    } else if rest == 0 {
        format!("{minutes} min")
    } else {
        format!("{minutes} min {rest} s")
    }
}

fn format_fps(fps: f64) -> String {
    if (fps - fps.round()).abs() < 1e-6 { format!("{fps:.0}") } else { format!("{fps:.2}") }
}

/// What stands between the project as it is and a clean upload.
pub fn check_platform_export(
    preset: &PlatformPreset,
    canvas: CanvasSize,
    fps: FrameRate,
    duration_seconds: f64,
    include_audio: bool,
) -> Vec<PlatformIssue> {
    let mut issues = Vec::new();
    let fps_value = fps.as_f64().unwrap_or(30.0);
    let preset_fps = preset.fps.as_f64().unwrap_or(30.0);
    if canvas != preset.canvas {
        issues.push(PlatformIssue {
            kind: PlatformIssueKind::Canvas,
            severity: PlatformSeverity::Warning,
            message: format!(
                "The canvas is {}×{} ({}); {} wants {}×{} ({}).",
                canvas.width,
                canvas.height,
                describe_aspect(canvas),
                preset.label,
                preset.canvas.width,
                preset.canvas.height,
                describe_aspect(preset.canvas)
            ),
            fix: Some(PlatformFix::Canvas(preset.canvas)),
        });
    }
    if (fps_value - preset_fps).abs() > 0.01 {
        issues.push(PlatformIssue {
            kind: PlatformIssueKind::Fps,
            severity: PlatformSeverity::Warning,
            message: format!(
                "The project runs at {} fps; {} recommends {}.",
                format_fps(fps_value),
                preset.label,
                format_fps(preset_fps)
            ),
            fix: Some(PlatformFix::Fps(preset.fps)),
        });
    }
    if let Some(max) = preset.max_duration_seconds {
        if duration_seconds > max + 0.001 {
            issues.push(PlatformIssue {
                kind: PlatformIssueKind::Duration,
                severity: PlatformSeverity::Error,
                message: format!(
                    "The timeline is {} long; {} accepts up to {}.",
                    format_duration(duration_seconds),
                    preset.label,
                    format_duration(max)
                ),
                fix: None,
            });
        }
    }
    if let Some(max) = preset.max_file_bytes {
        let estimate = estimate_platform_file_bytes(canvas, fps_value, duration_seconds, include_audio);
        if estimate > max {
            issues.push(PlatformIssue {
                kind: PlatformIssueKind::Size,
                severity: PlatformSeverity::Error,
                message: format!(
                    "The file would be about {}; {} accepts up to {}.",
                    format_bytes(estimate),
                    preset.label,
                    format_bytes(max)
                ),
                fix: None,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates_follow_youtubes_table_by_the_shorter_side() {
        assert_eq!(recommended_video_bitrate(CanvasSize::HD_1080, 30.0), 8_000_000);
        assert_eq!(recommended_video_bitrate(PORTRAIT, 30.0), 8_000_000);
        assert_eq!(recommended_video_bitrate(CanvasSize::HD_1080, 60.0), 12_000_000);
        assert_eq!(recommended_video_bitrate(CanvasSize { width: 1280, height: 720 }, 30.0), 5_000_000);
        assert_eq!(recommended_video_bitrate(CanvasSize { width: 3840, height: 2160 }, 60.0), 60_000_000);
        assert_eq!(recommended_video_bitrate(CanvasSize { width: 640, height: 360 }, 25.0), 1_000_000);
    }

    #[test]
    fn encode_settings_close_the_gop_every_half_second() {
        let settings = platform_encode_settings(PORTRAIT, 30.0);
        assert_eq!(
            settings,
            PlatformEncodeSettings { video_bitrate: 8_000_000, audio_bitrate: 384_000, gop_frames: 15 }
        );
        assert_eq!(platform_encode_settings(CanvasSize::HD_1080, 60.0).gop_frames, 30);
    }

    #[test]
    fn a_matching_project_has_no_issues() {
        let shorts = platform_preset("youtube-shorts").unwrap();
        assert!(check_platform_export(shorts, PORTRAIT, FrameRate::FPS_30, 45.0, true).is_empty());
    }

    #[test]
    fn mismatches_come_with_fixes() {
        let shorts = platform_preset("youtube-shorts").unwrap();
        let issues = check_platform_export(shorts, CanvasSize::HD_1080, FrameRate::FPS_60, 45.0, true);
        assert_eq!(
            issues.iter().map(|issue| issue.kind).collect::<Vec<_>>(),
            [PlatformIssueKind::Canvas, PlatformIssueKind::Fps]
        );
        assert_eq!(issues[0].fix, Some(PlatformFix::Canvas(PORTRAIT)));
        assert!(issues[0].message.contains("16:9") && issues[0].message.contains("9:16"));
        assert_eq!(issues[1].fix, Some(PlatformFix::Fps(FrameRate::FPS_30)));
        assert!(issues.iter().all(|issue| issue.severity == PlatformSeverity::Warning));
    }

    #[test]
    fn length_and_size_limits_are_errors() {
        let shorts = platform_preset("youtube-shorts").unwrap();
        let issues = check_platform_export(shorts, PORTRAIT, FrameRate::FPS_30, 200.0, true);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, PlatformIssueKind::Duration);
        assert_eq!(issues[0].severity, PlatformSeverity::Error);
        assert!(issues[0].message.contains("3 min 20 s"));

        let x = platform_preset("x").unwrap();
        let seven20 = CanvasSize { width: 1280, height: 720 };
        assert!(check_platform_export(x, seven20, FrameRate::FPS_30, 140.0, true).is_empty());
        let issues = check_platform_export(x, CanvasSize { width: 3840, height: 2160 }, FrameRate::FPS_60, 140.0, true);
        assert_eq!(issues.last().map(|issue| issue.kind), Some(PlatformIssueKind::Size));
        assert!(issues.last().unwrap().message.contains("512 MB"));
    }

    #[test]
    fn the_estimate_adds_audio_to_video() {
        assert_eq!(
            estimate_platform_file_bytes(CanvasSize::HD_1080, 30.0, 10.0, true),
            ((8_000_000u64 + 384_000) * 10) / 8
        );
    }

    #[test]
    fn safe_areas_stay_inside_portrait_frames() {
        for preset in PLATFORM_PRESETS {
            if let Some(area) = preset.safe_area {
                assert!(area.top + area.bottom < 1.0 && area.left + area.right < 1.0, "{}", preset.id);
                assert!(preset.canvas.height > preset.canvas.width, "{}", preset.id);
            }
        }
    }
}

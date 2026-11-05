/**
 * Platform export presets: the canvas, frame rate and limits each platform
 * publishes (or, where it publishes nothing, the figures third-party guides
 * agree on), the encoder settings YouTube documents for H.264 uploads, and
 * the part of a vertical frame the platform's own controls cover.
 *
 * Every preset says where its numbers come from. `verified: "official"`
 * means the platform's help pages (CONFIRMED); `"third-party"` means
 * guides and tools that may lag or disagree (INFERRED). Safe areas are
 * approximate everywhere: TikTok itself says the covered area depends on
 * the caption length, and every platform redraws its overlay now and then.
 *
 * The desktop editor keeps the same table in `crates/editor/src/platforms.rs`.
 */

import type { TCanvasSize } from "@/project/types";

export const PLATFORM_PRESET_IDS = [
	"youtube",
	"youtube-shorts",
	"tiktok",
	"instagram-reels",
	"linkedin",
	"x",
] as const;

export type PlatformPresetId = (typeof PLATFORM_PRESET_IDS)[number];

export function isPlatformPresetId(value: unknown): value is PlatformPresetId {
	return (
		typeof value === "string" &&
		PLATFORM_PRESET_IDS.some((id) => id === value)
	);
}

/**
 * Margins of the frame the platform's own interface covers, as fractions
 * of the canvas width (left, right) and height (top, bottom).
 */
export interface SafeArea {
	top: number;
	bottom: number;
	left: number;
	right: number;
}

export interface PlatformPreset {
	id: PlatformPresetId;
	label: string;
	/** Canvas size the platform recommends. */
	canvas: TCanvasSize;
	/** Frame rate the platform recommends. */
	fps: number;
	/** Longest upload in seconds, or null when none is published. */
	maxDurationSeconds: number | null;
	/** Largest upload in bytes, or null when none is published. */
	maxFileBytes: number | null;
	/** Part of the frame the platform's controls leave visible, if any. */
	safeArea: SafeArea | null;
	verified: "official" | "third-party";
	/** One line on where the figures come from. */
	source: string;
}

const GB = 1024 * 1024 * 1024;
const MB = 1024 * 1024;

export const PLATFORM_PRESETS: readonly PlatformPreset[] = [
	{
		id: "youtube",
		label: "YouTube",
		canvas: { width: 1920, height: 1080 },
		fps: 30,
		maxDurationSeconds: null,
		maxFileBytes: null,
		safeArea: null,
		verified: "official",
		source:
			"YouTube recommended upload encoding settings (support.google.com/youtube/answer/1722171)",
	},
	{
		id: "youtube-shorts",
		label: "YouTube Shorts",
		canvas: { width: 1080, height: 1920 },
		fps: 30,
		maxDurationSeconds: 180,
		maxFileBytes: null,
		// Third-party safe-zone tools converge on 288 px top, 672 px
		// bottom, 48 px left and 192 px right of a 1080×1920 frame.
		safeArea: { top: 0.15, bottom: 0.35, left: 0.045, right: 0.18 },
		verified: "third-party",
		source:
			"Shorts length from YouTube Help; safe area from third-party safe-zone tools (approximate)",
	},
	{
		id: "tiktok",
		label: "TikTok",
		canvas: { width: 1080, height: 1920 },
		fps: 30,
		maxDurationSeconds: 600,
		maxFileBytes: null,
		// Guides give 10 % top and 15 % bottom, with the icon column about
		// 150 px wide on the right. TikTok says the covered area depends
		// on the caption length.
		safeArea: { top: 0.1, bottom: 0.15, left: 0.04, right: 0.14 },
		verified: "third-party",
		source:
			"Third-party TikTok spec guides (approximate); TikTok publishes no bitrate or fixed safe zone",
	},
	{
		id: "instagram-reels",
		label: "Instagram Reels",
		canvas: { width: 1080, height: 1920 },
		fps: 30,
		maxDurationSeconds: 180,
		maxFileBytes: 4 * GB,
		// Guides keep about 250 px clear at the top and 350 px at the
		// bottom of a 1080×1920 frame, plus the icon column on the right.
		safeArea: { top: 0.13, bottom: 0.18, left: 0.04, right: 0.14 },
		verified: "third-party",
		source:
			"Meta help pages for size and length; safe area from third-party guides (approximate)",
	},
	{
		id: "linkedin",
		label: "LinkedIn",
		canvas: { width: 1920, height: 1080 },
		fps: 30,
		maxDurationSeconds: 900,
		maxFileBytes: 5 * GB,
		safeArea: null,
		verified: "official",
		source: "LinkedIn Help: native video up to 5 GB and 15 minutes from the web",
	},
	{
		id: "x",
		label: "X",
		canvas: { width: 1280, height: 720 },
		fps: 30,
		maxDurationSeconds: 140,
		maxFileBytes: 512 * MB,
		safeArea: null,
		verified: "third-party",
		source:
			"Third-party guides for standard accounts; X's own spec pages could not be reached",
	},
];

export function getPlatformPreset({
	id,
}: {
	id: string | null | undefined;
}): PlatformPreset | null {
	if (!id) return null;
	return PLATFORM_PRESETS.find((preset) => preset.id === id) ?? null;
}

/** Encoder settings for a platform upload. */
export interface PlatformEncodeSettings {
	/** H.264 target bitrate in bits per second, from YouTube's SDR table. */
	videoBitrate: number;
	/** AAC stereo bitrate in bits per second. */
	audioBitrate: number;
	/** Seconds between key frames: a closed GOP of half the frame rate. */
	keyFrameIntervalSeconds: number;
	/** WebCodecs codec string: H.264 High profile at the fitting level. */
	codec: string;
}

/** YouTube's recommended stereo AAC-LC bitrate. */
export const PLATFORM_AUDIO_BITRATE = 384_000;

/** YouTube's "closed GOP, with GOP set to half the frame rate". */
export const PLATFORM_KEY_FRAME_INTERVAL_SECONDS = 0.5;

/**
 * YouTube's recommended SDR upload bitrate (bits per second) for a frame
 * size and rate. The tier follows the frame's shorter side, so portrait
 * 1080×1920 is 1080p. Rates above 40 fps take the high-rate column.
 */
export function recommendedVideoBitrate({
	canvas,
	fps,
}: {
	canvas: TCanvasSize;
	fps: number;
}): number {
	const side = Math.min(canvas.width, canvas.height);
	const high = fps > 40;
	const megabits = (() => {
		if (side >= 4320) return high ? 180 : 120;
		if (side >= 2160) return high ? 60 : 40;
		if (side >= 1440) return high ? 24 : 16;
		if (side >= 1080) return high ? 12 : 8;
		if (side >= 720) return high ? 7.5 : 5;
		if (side >= 480) return high ? 4 : 2.5;
		return high ? 1.5 : 1;
	})();
	return Math.round(megabits * 1_000_000);
}

/**
 * H.264 levels by macroblocks per second and per frame (ITU-T H.264
 * table A-1), lowest first.
 */
const H264_LEVELS: readonly {
	hex: string;
	macroblocksPerSecond: number;
	macroblocksPerFrame: number;
}[] = [
	{ hex: "1f", macroblocksPerSecond: 108_000, macroblocksPerFrame: 3_600 },
	{ hex: "20", macroblocksPerSecond: 216_000, macroblocksPerFrame: 5_120 },
	{ hex: "28", macroblocksPerSecond: 245_760, macroblocksPerFrame: 8_192 },
	{ hex: "2a", macroblocksPerSecond: 522_240, macroblocksPerFrame: 8_704 },
	{ hex: "32", macroblocksPerSecond: 589_824, macroblocksPerFrame: 22_080 },
	{ hex: "33", macroblocksPerSecond: 983_040, macroblocksPerFrame: 36_864 },
	{ hex: "34", macroblocksPerSecond: 2_073_600, macroblocksPerFrame: 36_864 },
	{ hex: "3c", macroblocksPerSecond: 4_177_920, macroblocksPerFrame: 139_264 },
	{ hex: "3d", macroblocksPerSecond: 8_355_840, macroblocksPerFrame: 139_264 },
	{ hex: "3e", macroblocksPerSecond: 16_711_680, macroblocksPerFrame: 139_264 },
];

/** The WebCodecs string for H.264 High profile at the lowest level that fits. */
export function h264HighCodecString({
	canvas,
	fps,
}: {
	canvas: TCanvasSize;
	fps: number;
}): string {
	const macroblocksPerFrame =
		Math.ceil(canvas.width / 16) * Math.ceil(canvas.height / 16);
	const macroblocksPerSecond = macroblocksPerFrame * fps;
	const level =
		H264_LEVELS.find(
			(candidate) =>
				candidate.macroblocksPerFrame >= macroblocksPerFrame &&
				candidate.macroblocksPerSecond >= macroblocksPerSecond,
		) ?? H264_LEVELS[H264_LEVELS.length - 1];
	return `avc1.6400${level.hex}`;
}

export function platformEncodeSettings({
	canvas,
	fps,
}: {
	canvas: TCanvasSize;
	fps: number;
}): PlatformEncodeSettings {
	return {
		videoBitrate: recommendedVideoBitrate({ canvas, fps }),
		audioBitrate: PLATFORM_AUDIO_BITRATE,
		keyFrameIntervalSeconds: PLATFORM_KEY_FRAME_INTERVAL_SECONDS,
		codec: h264HighCodecString({ canvas, fps }),
	};
}

/** Rough size of the file the platform settings produce. */
export function estimatePlatformFileBytes({
	canvas,
	fps,
	durationSeconds,
	includeAudio,
}: {
	canvas: TCanvasSize;
	fps: number;
	durationSeconds: number;
	includeAudio: boolean;
}): number {
	const bits =
		recommendedVideoBitrate({ canvas, fps }) +
		(includeAudio ? PLATFORM_AUDIO_BITRATE : 0);
	return Math.round((bits * Math.max(durationSeconds, 0)) / 8);
}

export type PlatformIssueKind = "canvas" | "fps" | "duration" | "size";

export interface PlatformIssue {
	kind: PlatformIssueKind;
	/** "warning" is a mismatch the export still works with; "error" means the platform would refuse the upload. */
	severity: "warning" | "error";
	message: string;
	/** Project setting that would clear the issue, when one does. */
	fix?: { canvas?: TCanvasSize; fps?: number };
}

export function describeAspect({ canvas }: { canvas: TCanvasSize }): string {
	const divisor = gcd(canvas.width, canvas.height);
	return `${canvas.width / divisor}:${canvas.height / divisor}`;
}

function gcd(a: number, b: number): number {
	return b === 0 ? a : gcd(b, a % b);
}

export function formatBytes({ bytes }: { bytes: number }): string {
	if (bytes >= GB) return `${(bytes / GB).toFixed(bytes >= 10 * GB ? 0 : 1)} GB`;
	if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
	return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function formatDuration({ seconds }: { seconds: number }): string {
	const whole = Math.round(seconds);
	const minutes = Math.floor(whole / 60);
	const rest = whole % 60;
	if (minutes === 0) return `${rest} s`;
	return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
}

/** What stands between the project as it is and a clean upload. */
export function checkPlatformExport({
	preset,
	canvas,
	fps,
	durationSeconds,
	includeAudio = true,
}: {
	preset: PlatformPreset;
	canvas: TCanvasSize;
	fps: number;
	durationSeconds: number;
	includeAudio?: boolean;
}): PlatformIssue[] {
	const issues: PlatformIssue[] = [];
	if (
		canvas.width !== preset.canvas.width ||
		canvas.height !== preset.canvas.height
	) {
		issues.push({
			kind: "canvas",
			severity: "warning",
			message: `The canvas is ${canvas.width}×${canvas.height} (${describeAspect({ canvas })}); ${preset.label} wants ${preset.canvas.width}×${preset.canvas.height} (${describeAspect({ canvas: preset.canvas })}).`,
			fix: { canvas: { ...preset.canvas } },
		});
	}
	if (Math.abs(fps - preset.fps) > 0.01) {
		issues.push({
			kind: "fps",
			severity: "warning",
			message: `The project runs at ${formatFps({ fps })} fps; ${preset.label} recommends ${preset.fps}.`,
			fix: { fps: preset.fps },
		});
	}
	if (
		preset.maxDurationSeconds !== null &&
		durationSeconds > preset.maxDurationSeconds + 0.001
	) {
		issues.push({
			kind: "duration",
			severity: "error",
			message: `The timeline is ${formatDuration({ seconds: durationSeconds })} long; ${preset.label} accepts up to ${formatDuration({ seconds: preset.maxDurationSeconds })}.`,
		});
	}
	if (preset.maxFileBytes !== null) {
		const estimate = estimatePlatformFileBytes({
			canvas,
			fps,
			durationSeconds,
			includeAudio,
		});
		if (estimate > preset.maxFileBytes) {
			issues.push({
				kind: "size",
				severity: "error",
				message: `The file would be about ${formatBytes({ bytes: estimate })}; ${preset.label} accepts up to ${formatBytes({ bytes: preset.maxFileBytes })}.`,
			});
		}
	}
	return issues;
}

function formatFps({ fps }: { fps: number }): string {
	return Number.isInteger(fps) ? String(fps) : fps.toFixed(2);
}

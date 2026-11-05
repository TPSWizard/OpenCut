export {
	PLATFORM_PRESET_IDS,
	PLATFORM_PRESETS,
	PLATFORM_AUDIO_BITRATE,
	PLATFORM_KEY_FRAME_INTERVAL_SECONDS,
	checkPlatformExport,
	describeAspect,
	estimatePlatformFileBytes,
	formatBytes,
	formatDuration,
	getPlatformPreset,
	h264HighCodecString,
	isPlatformPresetId,
	platformEncodeSettings,
	recommendedVideoBitrate,
} from "./catalogue";
export type {
	PlatformEncodeSettings,
	PlatformIssue,
	PlatformIssueKind,
	PlatformPreset,
	PlatformPresetId,
	SafeArea,
} from "./catalogue";

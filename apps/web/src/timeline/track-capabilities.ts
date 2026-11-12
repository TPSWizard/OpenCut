import type {
	TimelineTrack,
	VideoTrack,
	AudioTrack,
	GraphicTrack,
	TextTrack,
	EffectTrack,
} from "@/timeline";

export function canTrackHaveAudio(
	track: TimelineTrack,
): track is VideoTrack | AudioTrack {
	return track.type === "audio" || track.type === "video";
}

export function canTrackBeHidden(
	track: TimelineTrack,
): track is VideoTrack | TextTrack | GraphicTrack | EffectTrack {
	return track.type !== "audio";
}

export function isTrackLocked(track: { locked?: boolean } | null | undefined): boolean {
	return Boolean(track?.locked);
}

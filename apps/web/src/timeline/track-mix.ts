/**
 * Track mixing: mute, solo, track volume (dB) and pan, through the shared
 * `trackmix` crate (WASM) exactly as on the desktop. Muted tracks are
 * silent; while any sounding track is soloed only soloed tracks are heard
 * (mute wins); volume runs from -60 dB (silent) to +12 dB and, with the
 * pan, applies after each clip's own.
 */

import { trackMix } from "opencut-wasm";
import { canTrackHaveAudio } from "./track-capabilities";
import type { SceneTracks, TimelineTrack } from "./types";

export const TRACK_VOLUME_MIN_DB = -60;
export const TRACK_VOLUME_MAX_DB = 12;

export interface TrackMix {
	audible: boolean;
	/** Linear gain; 0 when not audible. */
	gain: number;
	pan: number;
}

/** A track's mixer settings as stored (missing ones at their defaults). */
export function getTrackMixSettings(track: TimelineTrack): { muted: boolean; solo: boolean; volume: number; pan: number } {
	const sounding = canTrackHaveAudio(track);
	const value = (key: "volume" | "pan") => {
		const raw = sounding ? (track as { volume?: unknown; pan?: unknown })[key] : undefined;
		return typeof raw === "number" && Number.isFinite(raw) ? raw : 0;
	};
	return {
		muted: sounding && "muted" in track ? Boolean(track.muted) : false,
		solo: sounding ? Boolean((track as { solo?: unknown }).solo) : false,
		volume: value("volume"),
		pan: value("pan"),
	};
}

/** How each track plays, by track id. */
export function trackMixes(tracks: readonly TimelineTrack[]): Map<string, TrackMix> {
	const settings = tracks.map((track) => ({ ...getTrackMixSettings(track), sounding: canTrackHaveAudio(track) }));
	const mixes = JSON.parse(trackMix(JSON.stringify(settings))) as TrackMix[];
	return new Map(tracks.map((track, index) => [track.id, mixes[index]]));
}

/** Every track of a scene in mixing order. */
export function sceneTrackList(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

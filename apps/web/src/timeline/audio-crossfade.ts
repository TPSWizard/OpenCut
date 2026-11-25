/**
 * Audio crossfades between neighbouring clips on a track, through the
 * shared `crossfade` crate (WASM), exactly as on the desktop
 * (`editor::audio_crossfade`). A clip's `crossfade` param (seconds) and
 * `crossfadeCurve` (`linear` or `equalPower`) join it to the next audio or
 * video clip on its track. For playback both clips are stretched over the
 * cut into their spare source audio ("handles") and cross on the curve
 * (`withAudioCrossfades`, used by every audio path in `@/media/audio`);
 * the timeline itself never overlaps. The one-step command
 * (`planElementCrossfade`) trims to make handles where they are short and
 * moves the clips after left to close the gap.
 */

import { audioCrossfades, planCrossfade } from "opencut-wasm";
import type { SceneTracks, TimelineElement } from "./types";
import { roundMediaTime } from "@/wasm";

/** The outgoing clip's crossfade length, seconds. */
export const CROSSFADE_PARAM = "crossfade";
/** `linear` or `equalPower`. */
export const CROSSFADE_CURVE_PARAM = "crossfadeCurve";
/** Longest crossfade, seconds. */
export const CROSSFADE_MAX_SECONDS = 10;
const TICKS = 120_000;

export type CrossfadeCurve = "linear" | "equalPower";

/** A fade's gain at `progress` (0 silent, 1 full) on its curve; the crate's `Curve::gain`. */
export function curveGain(curve: CrossfadeCurve, progress: number): number {
	const p = Number.isFinite(progress) ? Math.min(1, Math.max(0, progress)) : 0;
	return curve === "equalPower" ? Math.sin((p * Math.PI) / 2) : p;
}

export function asCurve(value: unknown): CrossfadeCurve {
	return value === "equalPower" ? "equalPower" : "linear";
}

/** A clip's crossfade into the next one: seconds (0 for none) and curve (equal power unless `crossfadeCurve` is `linear`). */
export function getClipCrossfade(element: { params: Record<string, unknown> }): { seconds: number; curve: CrossfadeCurve } {
	const raw = element.params[CROSSFADE_PARAM];
	const seconds = typeof raw === "number" && Number.isFinite(raw) ? Math.min(CROSSFADE_MAX_SECONDS, Math.max(0, raw)) : 0;
	return { seconds, curve: element.params[CROSSFADE_CURVE_PARAM] === "linear" ? "linear" : "equalPower" };
}

function isRetimed(element: TimelineElement): boolean {
	const retime = "retime" in element ? element.retime : undefined;
	if (!retime) return false;
	return retime.rate !== 1 || retime.reverse === true || (retime.ramp?.length ?? 0) > 0;
}

function ownFades(element: TimelineElement): { fadeIn: number; fadeOut: number } {
	// As `getAudioFades` (audio-state), without importing it here (it imports this module).
	const finite = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
	const length = Math.max(0, element.duration / TICKS);
	let fadeIn = Math.min(10, Math.max(0, finite(element.params.fadeIn)));
	let fadeOut = Math.min(10, Math.max(0, finite(element.params.fadeOut)));
	if (fadeIn + fadeOut > length && fadeIn + fadeOut > 0) {
		const scale = length / (fadeIn + fadeOut);
		fadeIn *= scale;
		fadeOut *= scale;
	}
	return { fadeIn, fadeOut };
}

/** The track's clips for the shared crate, in the track's order. */
export function crossfadeTrackClips(elements: readonly TimelineElement[]) {
	return elements.map((element) => {
		const { fadeIn, fadeOut } = ownFades(element);
		const { seconds, curve } = getClipCrossfade(element);
		const hidden = "hidden" in element && element.hidden === true;
		return {
			start: element.startTime,
			duration: element.duration,
			trimStart: element.trimStart,
			trimEnd: element.trimEnd,
			retimed: isRetimed(element),
			eligible: !hidden && (element.type === "audio" || element.type === "video"),
			fadeIn,
			fadeOut,
			crossfade: Math.round(seconds * TICKS),
			curve,
		};
	});
}

interface PlayedClip {
	start: number;
	duration: number;
	trimStart: number;
	trimEnd: number;
	fadeIn: number;
	fadeOut: number;
	fadeInCurve: CrossfadeCurve;
	fadeOutCurve: CrossfadeCurve;
}

/**
 * The elements as they play with their crossfades: a joined clip is
 * stretched over the cut, its fades in `fadeIn` / `fadeOut` and their
 * curves in `fadeInCurve` / `fadeOutCurve`. Untouched elements are returned
 * as they are; the same array comes back when no crossfade is set.
 */
export function crossfadedElements<T extends TimelineElement>(elements: readonly T[]): T[] {
	if (!elements.some((element) => getClipCrossfade(element).seconds > 0)) return elements as T[];
	const played = JSON.parse(audioCrossfades(JSON.stringify(crossfadeTrackClips(elements)))) as (PlayedClip | null)[];
	return elements.map((element, index) => {
		const clip = played[index];
		if (!clip) return element;
		return {
			...element,
			startTime: roundMediaTime({ time: clip.start }),
			duration: roundMediaTime({ time: clip.duration }),
			trimStart: roundMediaTime({ time: clip.trimStart }),
			trimEnd: roundMediaTime({ time: clip.trimEnd }),
			params: {
				...element.params,
				fadeIn: clip.fadeIn,
				fadeOut: clip.fadeOut,
				fadeInCurve: clip.fadeInCurve,
				fadeOutCurve: clip.fadeOutCurve,
			},
		};
	});
}

/** Every track with its crossfades applied, for the audio paths. */
export function withAudioCrossfades(tracks: SceneTracks): SceneTracks {
	const apply = <T extends { elements: TimelineElement[] }>(track: T): T => {
		const elements = crossfadedElements(track.elements);
		return elements === track.elements ? track : { ...track, elements };
	};
	return { overlay: tracks.overlay.map(apply), main: apply(tracks.main), audio: tracks.audio.map(apply) };
}

export interface ClipEdit {
	start: number;
	duration: number;
	trimStart: number;
	trimEnd: number;
}

export interface CrossfadePlan {
	/** The incoming clip's index on the track. */
	to: number;
	/** The crossfade to store, ticks. */
	length: number;
	trimOut: number;
	trimIn: number;
	/** How far the incoming clip and those after it move left, ticks. */
	shift: number;
	fromEdit: ClipEdit;
	toEdit: ClipEdit;
	/** The other clips after the incoming one, each moved `shift` earlier. */
	moved: number[];
}

/**
 * The one-step crossfade of `seconds` from clip `index` into the next on
 * its track: the length it gets and the trims and moves that make room.
 * Throws with the crate's message when it cannot be made.
 */
export function planElementCrossfade(elements: readonly TimelineElement[], index: number, seconds: number): CrossfadePlan {
	const ticks = Math.round(Math.min(CROSSFADE_MAX_SECONDS, Math.max(0, Number.isFinite(seconds) ? seconds : 0)) * TICKS);
	const result = JSON.parse(planCrossfade(JSON.stringify(crossfadeTrackClips(elements)), index, ticks)) as
		| { ok: CrossfadePlan }
		| { error: string; message: string };
	if ("error" in result) throw new Error(result.message);
	return result.ok;
}

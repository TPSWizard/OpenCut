import { hasKeyframesForPath } from "@/animation/keyframe-query";
import { resolveNumberAtTime } from "@/animation/values";
import { VOLUME_DB_MAX, VOLUME_DB_MIN } from "./audio-constants";
import type { TimelineElement } from "./types";
const DEFAULT_STEP_SECONDS = 1 / 60;

export type AudioCapableElement = Extract<
	TimelineElement,
	{ type: "audio" | "video" }
>;

export function clampDb(value: number): number {
	if (!Number.isFinite(value)) {
		return 0;
	}

	return Math.min(VOLUME_DB_MAX, Math.max(VOLUME_DB_MIN, value));
}

export function dBToLinear(db: number): number {
	return 10 ** (clampDb(db) / 20);
}

export function getElementVolume({
	element,
}: {
	element: AudioCapableElement;
}): number {
	const value = element.params.volume;
	return typeof value === "number" ? value : 0;
}

export function isElementMuted({
	element,
}: {
	element: AudioCapableElement;
}): boolean {
	return element.params.muted === true;
}

/** Longest fade in or out, in seconds. */
export const AUDIO_FADE_MAX_SECONDS = 10;

function finiteParam(value: unknown): number {
	return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/**
 * The clip's fade in and fade out in seconds (`fadeIn` / `fadeOut` params),
 * each within 0 to 10 s and scaled down together when they would overlap
 * on a short clip. The desktop's `AudioClipPlan` reads the same params.
 */
export function getAudioFades({ element }: { element: AudioCapableElement }): { fadeIn: number; fadeOut: number } {
	const length = Math.max(0, element.duration / TICKS_PER_SECOND);
	let fadeIn = Math.min(AUDIO_FADE_MAX_SECONDS, Math.max(0, finiteParam(element.params.fadeIn)));
	let fadeOut = Math.min(AUDIO_FADE_MAX_SECONDS, Math.max(0, finiteParam(element.params.fadeOut)));
	if (fadeIn + fadeOut > length && fadeIn + fadeOut > 0) {
		const scale = length / (fadeIn + fadeOut);
		fadeIn *= scale;
		fadeOut *= scale;
	}
	return { fadeIn, fadeOut };
}

export function hasAudioFades({ element }: { element: AudioCapableElement }): boolean {
	const { fadeIn, fadeOut } = getAudioFades({ element });
	return fadeIn > 0 || fadeOut > 0;
}

/** The fade's linear gain (0 to 1) `localTime` seconds into the clip: a straight ramp up, then down. */
export function fadeGainAt({ element, localTime }: { element: AudioCapableElement; localTime: number }): number {
	const { fadeIn, fadeOut } = getAudioFades({ element });
	const length = element.duration / TICKS_PER_SECOND;
	let gain = 1;
	if (fadeIn > 0 && localTime < fadeIn) gain *= Math.max(0, localTime) / fadeIn;
	if (fadeOut > 0 && localTime > length - fadeOut) gain *= Math.max(0, length - localTime) / fadeOut;
	return Math.min(1, Math.max(0, gain));
}

/** Stereo pan from -1 (left) to 1 (right); 0 leaves the sound centred. */
export function getElementPan({ element }: { element: AudioCapableElement }): number {
	return Math.min(1, Math.max(-1, finiteParam(element.params.pan)));
}

/**
 * Pans one stereo frame as Web Audio's StereoPannerNode does for two
 * channels (equal power; at 0 nothing changes), so the export matches the
 * preview. The desktop's `editor::pan_stereo` is the same formula.
 */
export function panStereo({ pan, left, right }: { pan: number; left: number; right: number }): [number, number] {
	if (pan === 0) return [left, right];
	const x = (pan <= 0 ? pan + 1 : pan) * (Math.PI / 2);
	const gainLeft = Math.cos(x);
	const gainRight = Math.sin(x);
	return pan <= 0 ? [left + right * gainLeft, right * gainRight] : [left * gainLeft, right + left * gainRight];
}

/** Whether the clip's gain changes over time (volume keys or fades). */
export function hasGainAutomation({ element }: { element: AudioCapableElement }): boolean {
	return hasAnimatedVolume({ element }) || hasAudioFades({ element });
}

export function hasAnimatedVolume({
	element,
}: {
	element: AudioCapableElement;
}): boolean {
	return hasKeyframesForPath({
		animations: element.animations,
		propertyPath: "volume",
	});
}

import { TICKS_PER_SECOND } from "@/wasm";

export function resolveEffectiveAudioGain({
	element,
	trackMuted = false,
	localTime,
}: {
	element: AudioCapableElement;
	trackMuted?: boolean;
	localTime: number;
}): number {
	if (trackMuted || isElementMuted({ element })) {
		return 0;
	}

	const resolvedDb = resolveNumberAtTime({
		baseValue: getElementVolume({ element }),
		animations: element.animations,
		propertyPath: "volume",
		localTime: Math.round(localTime * TICKS_PER_SECOND),
	});

	return dBToLinear(resolvedDb) * fadeGainAt({ element, localTime });
}

export function buildWaveformGainSamples({
	element,
	count,
}: {
	element: AudioCapableElement;
	count: number;
}): number[] {
	const durationSeconds = element.duration / TICKS_PER_SECOND;
	return Array.from({ length: count }, (_, i) => {
		const localTime = ((i + 0.5) / count) * durationSeconds;
		return resolveEffectiveAudioGain({ element, localTime });
	});
}

export function buildAudioGainAutomation({
	element,
	trackMuted = false,
	fromLocalTime,
	toLocalTime,
	stepSeconds = DEFAULT_STEP_SECONDS,
}: {
	element: AudioCapableElement;
	trackMuted?: boolean;
	fromLocalTime: number;
	toLocalTime: number;
	stepSeconds?: number;
}): Array<{ localTime: number; gain: number }> {
	const startTime = Math.max(0, fromLocalTime);
	const endTime = Math.max(startTime, toLocalTime);
	const safeStep =
		Number.isFinite(stepSeconds) && stepSeconds > 0
			? stepSeconds
			: DEFAULT_STEP_SECONDS;
	const points: Array<{ localTime: number; gain: number }> = [];

	for (let localTime = startTime; localTime < endTime; localTime += safeStep) {
		points.push({
			localTime,
			gain: resolveEffectiveAudioGain({
				element,
				trackMuted,
				localTime,
			}),
		});
	}

	points.push({
		localTime: endTime,
		gain: resolveEffectiveAudioGain({
			element,
			trackMuted,
			localTime: endTime,
		}),
	});

	return points;
}

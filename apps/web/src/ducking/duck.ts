import type { EditorCore } from "@/core";
import type { ScalarChannel } from "@/animation/types";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import {
	collectAudioElements,
	createAudioContext,
	type CollectedAudioElement,
} from "@/media/audio";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import {
	getElementVolume,
	type AudioCapableElement,
} from "@/timeline/audio-state";
import { canElementHaveAudio } from "@/timeline/element-utils";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import {
	DUCKING_DEFAULTS,
	buildDuckingKeys,
	meanSquareToDb,
	speechIntervals,
	totalSeconds,
	type DuckingOptions,
	type Interval,
} from "./envelope";

/** Sample rate the voice is measured at; speech needs no more. */
const ANALYSIS_SAMPLE_RATE = 16_000;

export interface DuckingResult {
	/** Keys written to the music's volume channel. */
	keyframes: number;
	/** Where speech was found, in timeline seconds. */
	intervals: Interval[];
	speechSeconds: number;
	/** Elements the voice was measured from. */
	voiceElementIds: string[];
}

export interface DuckingTarget {
	trackId: string;
	element: AudioCapableElement;
}

/** The audio-carrying elements of the active scene with their tracks. */
export function listAudioElements({
	tracks,
}: {
	tracks: SceneTracks;
}): DuckingTarget[] {
	const all: TimelineTrack[] = [...tracks.overlay, tracks.main, ...tracks.audio];
	const targets: DuckingTarget[] = [];
	for (const track of all) {
		for (const element of track.elements as TimelineElement[]) {
			if (canElementHaveAudio(element)) {
				targets.push({ trackId: track.id, element });
			}
		}
	}
	return targets;
}

/**
 * Measures the voice over the music clip's span and writes volume keys
 * that dip the music while speech plays. `voiceElementIds` restricts the
 * voice to those elements; by default every other audible element counts.
 */
export async function duckElement({
	editor,
	trackId,
	elementId,
	voiceElementIds,
	options = {},
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	voiceElementIds?: string[];
	options?: Partial<DuckingOptions>;
}): Promise<DuckingResult> {
	const settings: DuckingOptions = { ...DUCKING_DEFAULTS, ...options };
	const tracks = editor.scenes.getActiveScene().tracks;
	const target = listAudioElements({ tracks }).find(
		(candidate) => candidate.trackId === trackId && candidate.element.id === elementId,
	);
	if (!target) {
		throw new Error("The element to duck is not an audio or video clip");
	}
	const music = target.element;

	const audioContext = createAudioContext({ sampleRate: ANALYSIS_SAMPLE_RATE });
	let collected: CollectedAudioElement[];
	try {
		collected = await collectAudioElements({
			tracks,
			mediaAssets: editor.media.getAssets(),
			audioContext,
		});
	} finally {
		void audioContext.close();
	}
	const voices = collected.filter((candidate) => {
		const id = candidate.timelineElement.id;
		if (id === music.id) return false;
		if (candidate.muted) return false;
		return voiceElementIds ? voiceElementIds.includes(id) : true;
	});
	if (voices.length === 0) {
		throw new Error(
			voiceElementIds
				? "None of the chosen voice clips has audio"
				: "There is no other audio clip to duck under",
		);
	}

	const clipStart = music.startTime / TICKS_PER_SECOND;
	const clipDuration = music.duration / TICKS_PER_SECOND;
	const levels = voiceLevels({
		voices,
		from: clipStart,
		to: clipStart + clipDuration,
		windowSeconds: settings.windowSeconds,
	});
	const intervals = speechIntervals({
		levels,
		windowSeconds: settings.windowSeconds,
		thresholdDb: settings.thresholdDb,
		holdSeconds: settings.holdSeconds,
		minSpeechSeconds: settings.minSpeechSeconds,
		offsetSeconds: clipStart,
	});
	const keys = buildDuckingKeys({
		intervals,
		clipStart,
		clipDuration,
		baseDb: getElementVolume({ element: music }),
		amountDb: settings.amountDb,
		attackSeconds: settings.attackSeconds,
		releaseSeconds: settings.releaseSeconds,
	});

	const channel: ScalarChannel | undefined =
		keys.length > 0
			? {
					keys: keys.map((key) => ({
						id: generateUUID(),
						time: roundMediaTime({ time: key.time * TICKS_PER_SECOND }),
						value: key.value,
						segmentToNext: "linear",
						tangentMode: "auto",
					})),
				}
			: undefined;
	editor.command.execute({
		command: new SetChannelCommand({
			trackId,
			elementId,
			propertyPath: "volume",
			channel,
		}),
	});

	return {
		keyframes: keys.length,
		intervals,
		speechSeconds: totalSeconds({ intervals }),
		voiceElementIds: voices.map((voice) => voice.timelineElement.id),
	};
}

/** Drops the element's volume keys; its own volume applies again. */
export function clearDucking({
	editor,
	trackId,
	elementId,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
}): void {
	editor.command.execute({
		command: new SetChannelCommand({
			trackId,
			elementId,
			propertyPath: "volume",
			channel: undefined,
		}),
	});
}

/**
 * RMS level per window of timeline time, summing every voice clip that
 * plays in the window. Trims and speed changes are followed; a clip's own
 * volume is not, since speech is detected on the source.
 */
function voiceLevels({
	voices,
	from,
	to,
	windowSeconds,
}: {
	voices: CollectedAudioElement[];
	from: number;
	to: number;
	windowSeconds: number;
}): Float64Array {
	const count = Math.max(0, Math.ceil((to - from) / windowSeconds));
	const levels = new Float64Array(count);
	for (let window = 0; window < count; window++) {
		const windowStart = from + window * windowSeconds;
		const windowEnd = Math.min(to, windowStart + windowSeconds);
		let energy = 0;
		let samples = 0;
		for (const voice of voices) {
			const clipFrom = Math.max(windowStart, voice.startTime);
			const clipTo = Math.min(windowEnd, voice.startTime + voice.duration);
			if (clipTo <= clipFrom) continue;
			const sourceFrom =
				voice.trimStart +
				getSourceTimeAtClipTime({
					clipTime: clipFrom - voice.startTime,
					retime: voice.retime,
				});
			const sourceTo =
				voice.trimStart +
				getSourceTimeAtClipTime({
					clipTime: clipTo - voice.startTime,
					retime: voice.retime,
				});
			const { buffer } = voice;
			const first = Math.max(0, Math.floor(sourceFrom * buffer.sampleRate));
			const last = Math.min(buffer.length, Math.ceil(sourceTo * buffer.sampleRate));
			if (last <= first) continue;
			const channels = buffer.numberOfChannels;
			for (let channel = 0; channel < channels; channel++) {
				const data = buffer.getChannelData(channel);
				for (let index = first; index < last; index++) {
					energy += data[index] * data[index];
				}
			}
			samples += (last - first) * channels;
		}
		levels[window] = meanSquareToDb({
			meanSquare: samples > 0 ? energy / samples : 0,
		});
	}
	return levels;
}

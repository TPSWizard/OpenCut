/**
 * Loudness of one clip's audio as it plays (its trimmed, retimed source
 * span, before its own volume), and normalising it by setting the clip's
 * volume so the integrated loudness meets a target.
 */

import type { EditorCore } from "@/core";
import { collectAudioElements, createAudioContext } from "@/media/audio";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import { clampDb, getElementVolume, hasAnimatedVolume } from "@/timeline/audio-state";
import type { TimelineElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { listAudioElements } from "@/ducking/duck";
import {
	DEFAULT_LOUDNESS_TARGET,
	measureLoudness,
	normalizationGain,
	type LoudnessReport,
} from "./r128";

/** Measured at the rate the standard's figures are given for. */
const ANALYSIS_SAMPLE_RATE = 48_000;

export async function measureElementLoudness({
	editor,
	trackId,
	elementId,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
}): Promise<LoudnessReport> {
	const tracks = editor.scenes.getActiveScene().tracks;
	const target = listAudioElements({ tracks }).find(
		(candidate) => candidate.trackId === trackId && candidate.element.id === elementId,
	);
	if (!target) {
		throw new Error("The element is not an audio or video clip");
	}
	const audioContext = createAudioContext({ sampleRate: ANALYSIS_SAMPLE_RATE });
	let collected: Awaited<ReturnType<typeof collectAudioElements>>;
	try {
		collected = await collectAudioElements({
			tracks,
			mediaAssets: editor.media.getAssets(),
			audioContext,
		});
	} finally {
		void audioContext.close();
	}
	const clip = collected.find((candidate) => candidate.timelineElement.id === elementId);
	if (!clip) {
		throw new Error("The clip has no audio to measure");
	}
	const { buffer } = clip;
	const sourceFrom = clip.trimStart + getSourceTimeAtClipTime({ clipTime: 0, retime: clip.retime });
	const sourceTo = clip.trimStart + getSourceTimeAtClipTime({ clipTime: clip.duration, retime: clip.retime });
	const first = Math.max(0, Math.floor(sourceFrom * buffer.sampleRate));
	const last = Math.min(buffer.length, Math.ceil(sourceTo * buffer.sampleRate));
	if (last <= first) {
		throw new Error("The clip's span holds no audio");
	}
	const channels: Float32Array[] = [];
	for (let channel = 0; channel < buffer.numberOfChannels; channel++) {
		channels.push(buffer.getChannelData(channel).slice(first, last));
	}
	return measureLoudness({ channels, sampleRate: buffer.sampleRate });
}

export interface NormalizeResult {
	report: LoudnessReport;
	/** The gain applied relative to the source, which becomes the clip's volume. */
	gainDb: number;
	volumeDb: number;
	previousVolumeDb: number;
	peakLimited: boolean;
	/** True when the clip has volume keyframes, which still apply on top. */
	animatedVolume: boolean;
}

/**
 * Measures the clip and sets its volume so its integrated loudness meets
 * `targetLufs`, held back so its true peak stays under `truePeakDbtp`.
 * The volume is clamped to the editor's range.
 */
export async function normalizeElementLoudness({
	editor,
	trackId,
	elementId,
	targetLufs = DEFAULT_LOUDNESS_TARGET.lufs,
	truePeakDbtp = DEFAULT_LOUDNESS_TARGET.truePeakDbtp,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	targetLufs?: number;
	truePeakDbtp?: number;
}): Promise<NormalizeResult> {
	const report = await measureElementLoudness({ editor, trackId, elementId });
	const gain = normalizationGain({ report, targetLufs, truePeakDbtp });
	if (!gain) {
		throw new Error("The clip is silent; there is nothing to normalise");
	}
	const element = listAudioElements({ tracks: editor.scenes.getActiveScene().tracks }).find(
		(candidate) => candidate.element.id === elementId,
	)?.element;
	if (!element) {
		throw new Error("The clip is gone");
	}
	const previousVolumeDb = getElementVolume({ element });
	const volumeDb = clampDb(gain.gainDb);
	editor.timeline.updateElements({
		updates: [
			{
				trackId,
				elementId,
				patch: { params: { ...element.params, volume: volumeDb } } as Partial<TimelineElement>,
			},
		],
	});
	return {
		report,
		gainDb: gain.gainDb,
		volumeDb,
		previousVolumeDb,
		peakLimited: gain.peakLimited,
		animatedVolume: hasAnimatedVolume({ element }),
	};
}

export { TICKS_PER_SECOND };

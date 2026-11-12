/**
 * Beat markers for a clip: its audio (the trimmed source span, mixed to
 * mono) is beat-tracked, the beats mapped onto the timeline, and placed
 * as bookmarks, which the timeline already snaps to.
 */

import type { EditorCore } from "@/core";
import { listAudioElements } from "@/ducking/duck";
import { collectAudioElements, createAudioContext } from "@/media/audio";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import { TICKS_PER_SECOND, mediaTime } from "@/wasm";
import { trackBeats, type BeatAnalysis, type BeatOptions } from "./track";

const ANALYSIS_SAMPLE_RATE = 16_000;

export interface BeatMarker {
	sourceSeconds: number;
	/** On the timeline, in ticks; `null` when outside the clip. */
	timelineTicks: number | null;
}

export interface ClipBeats extends BeatAnalysis {
	markers: BeatMarker[];
}

/** Beat-tracks the clip's source span. */
export async function analyzeClipBeats({
	editor,
	trackId,
	elementId,
	options = {},
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<BeatOptions>;
}): Promise<ClipBeats> {
	const tracks = editor.scenes.getActiveScene().tracks;
	const target = listAudioElements({ tracks }).find(
		(candidate) => candidate.trackId === trackId && candidate.element.id === elementId,
	);
	if (!target) {
		throw new Error("Beat detection needs an audio or video clip");
	}
	const audioContext = createAudioContext({ sampleRate: ANALYSIS_SAMPLE_RATE });
	let collected: Awaited<ReturnType<typeof collectAudioElements>>;
	try {
		collected = await collectAudioElements({ tracks, mediaAssets: editor.media.getAssets(), audioContext });
	} finally {
		void audioContext.close();
	}
	const clip = collected.find((candidate) => candidate.timelineElement.id === elementId);
	if (!clip) {
		throw new Error("The clip has no audio to analyse");
	}
	const { buffer } = clip;
	const sourceFrom = clip.trimStart;
	const sourceTo = clip.trimStart + Math.max(getSourceTimeAtClipTime({ clipTime: 0, retime: clip.retime }), getSourceTimeAtClipTime({ clipTime: clip.duration, retime: clip.retime }));
	const first = Math.max(0, Math.floor(sourceFrom * buffer.sampleRate));
	const last = Math.min(buffer.length, Math.ceil(sourceTo * buffer.sampleRate));
	if (last <= first) {
		throw new Error("The clip's span holds no audio");
	}
	const mono = new Float32Array(last - first);
	for (let channel = 0; channel < buffer.numberOfChannels; channel++) {
		const data = buffer.getChannelData(channel);
		for (let index = first; index < last; index++) mono[index - first] += data[index] / buffer.numberOfChannels;
	}
	const analysis = trackBeats({ samples: mono, sampleRate: buffer.sampleRate, options });
	const element = target.element;
	const markers = analysis.beats.map((beat) => {
		const sourceSeconds = sourceFrom + beat;
		const offset = Math.round(sourceSeconds * TICKS_PER_SECOND) - element.trimStart;
		return {
			sourceSeconds,
			timelineTicks: offset > 0 && offset < element.duration ? element.startTime + offset : null,
		};
	});
	return { ...analysis, markers };
}

/** Every `every`-th beat's timeline time, in ticks, from the first beat. */
export function beatTimes({ markers, every }: { markers: BeatMarker[]; every: number }): number[] {
	const step = Math.max(1, Math.round(every));
	return markers
		.filter((marker, index) => index % step === 0 && marker.timelineTicks !== null)
		.map((marker) => marker.timelineTicks as number);
}

/** Bookmarks the beats that are not bookmarked yet; returns how many were added. */
export function bookmarkBeats({ editor, times }: { editor: EditorCore; times: number[] }): number {
	let added = 0;
	for (const ticks of times) {
		const time = mediaTime({ ticks });
		if (editor.scenes.isBookmarked({ time })) continue;
		void editor.scenes.toggleBookmark({ time });
		added += 1;
	}
	return added;
}

/** Removes the bookmarks at the given times; returns how many went. */
export function unbookmarkBeats({ editor, times }: { editor: EditorCore; times: number[] }): number {
	let removed = 0;
	for (const ticks of times) {
		const time = mediaTime({ ticks });
		if (!editor.scenes.isBookmarked({ time })) continue;
		void editor.scenes.removeBookmark({ time });
		removed += 1;
	}
	return removed;
}

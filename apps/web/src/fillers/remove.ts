/**
 * Finding and cutting the filler words of a clip. The timed words come
 * from caption elements that already cover the clip (word-timed captions
 * from the transcriber or an import), or from a fresh transcription of
 * the scene; the cuts go through the same command as silence removal.
 */

import { RemoveRangesCommand } from "@/commands/timeline/element/remove-ranges";
import type { EditorCore } from "@/core";
import { generateCaptions } from "@/captions/generate";
import type { RippleScope } from "@/silence/detect";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import type { TranscriptionLanguage, TranscriptionModelId } from "@/transcription/types";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import {
	fillerIntervals,
	findFillers,
	totalSeconds,
	type FillerHit,
	type FillerInterval,
	type FillerOptions,
	type TimedWordLike,
} from "./detect";

export type FillerWordSource = "captions" | "transcribe";

export interface FillerReport {
	/** Where the words came from. */
	source: FillerWordSource;
	/** The language the fillers were matched against. */
	language: string;
	words: number;
	fillers: FillerHit[];
	/** Timeline seconds to cut. */
	intervals: FillerInterval[];
	seconds: number;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

function clipSpan({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): { from: number; to: number } {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = allTracks(tracks).find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!track || !element || !(element.type === "video" || element.type === "audio")) {
		throw new Error("Filler removal needs an audio or video clip");
	}
	return { from: element.startTime / TICKS_PER_SECOND, to: (element.startTime + element.duration) / TICKS_PER_SECOND };
}

/** Timed words of the visible caption elements that overlap `[from, to]`, on the timeline clock. */
export function wordsFromCaptions({ tracks, from, to }: { tracks: SceneTracks; from: number; to: number }): TimedWordLike[] {
	const words: TimedWordLike[] = [];
	for (const track of allTracks(tracks)) {
		if (track.type !== "text" || track.hidden) continue;
		for (const element of track.elements as TimelineElement[]) {
			if (element.type !== "text" || element.hidden || !element.words?.length) continue;
			const base = element.startTime / TICKS_PER_SECOND;
			for (const word of element.words) {
				const start = base + word.start / TICKS_PER_SECOND;
				const end = base + word.end / TICKS_PER_SECOND;
				if (end >= from && start <= to) words.push({ text: word.text, start, end });
			}
		}
	}
	return words.sort((a, b) => a.start - b.start);
}

/** Finds the clip's fillers; transcribes the scene when no captions cover the clip. */
export async function findClipFillers({
	editor,
	trackId,
	elementId,
	language = "auto",
	source = "auto",
	modelId,
	options = {},
	onStep,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	/** Filler list to use; `auto` takes the transcription's language when it transcribes, else the safe set. */
	language?: string;
	source?: FillerWordSource | "auto";
	modelId?: TranscriptionModelId;
	options?: Partial<FillerOptions>;
	onStep?: (step: string) => void;
}): Promise<FillerReport> {
	const { from, to } = clipSpan({ editor, trackId, elementId });
	let words: TimedWordLike[] = [];
	let used: FillerWordSource = "captions";
	let matchLanguage = language;
	if (source !== "transcribe") {
		words = wordsFromCaptions({ tracks: editor.scenes.getActiveScene().tracks, from, to });
	}
	if (words.length === 0 && source !== "captions") {
		onStep?.("Transcribing…");
		const { result } = await generateCaptions({
			editor,
			language: (language === "auto" ? "auto" : language) as TranscriptionLanguage,
			modelId,
		});
		words = result.words.filter((word) => word.end >= from && word.start <= to).map((word) => ({ text: word.text, start: word.start, end: word.end }));
		used = "transcribe";
		if (language === "auto" && result.language) matchLanguage = result.language;
	}
	const fillers = findFillers({ words, language: matchLanguage, options });
	const intervals = fillerIntervals({ words, fillers, options })
		.map((interval) => ({ start: Math.max(from, interval.start), end: Math.min(to, interval.end) }))
		.filter((interval) => interval.end > interval.start);
	return { source: used, language: matchLanguage, words: words.length, fillers, intervals, seconds: totalSeconds({ intervals }) };
}

/** Cuts the intervals out of the clip in one undo step; returns the pieces left. */
export function cutIntervals({
	editor,
	trackId,
	elementId,
	intervals,
	ripple = "all",
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	intervals: FillerInterval[];
	ripple?: RippleScope;
}): { trackId: string; elementId: string }[] {
	if (intervals.length === 0) return [{ trackId, elementId }];
	const command = new RemoveRangesCommand({
		trackId,
		elementId,
		ranges: intervals.map((interval) => ({
			start: roundMediaTime({ time: interval.start * TICKS_PER_SECOND }),
			end: roundMediaTime({ time: interval.end * TICKS_PER_SECOND }),
		})),
		ripple,
	});
	const rippleMode = editor.command.isRippleEnabled;
	editor.command.isRippleEnabled = false;
	try {
		editor.command.execute({ command });
	} finally {
		editor.command.isRippleEnabled = rippleMode;
	}
	return command.getRemainingElements();
}

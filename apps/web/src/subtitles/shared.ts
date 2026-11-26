import { parseSubtitles } from "opencut-wasm";
import type { ParseSubtitleResult, SubtitleCue } from "./types";

/**
 * SRT and WebVTT through the shared `subtitleparse` crate (WASM), the same
 * as the desktop's subtitle import: cues with their text and times, and
 * WebVTT word timestamps (as both editors' VTT export writes them) as the
 * cue's timed words.
 */
export function parseSharedSubtitles({ input, fileName }: { input: string; fileName: string }): ParseSubtitleResult {
	const parsed = JSON.parse(parseSubtitles(input, fileName)) as {
		cues: { text: string; start: number; end: number; words?: { text: string; start: number; end: number }[] }[];
		skipped: number;
	};
	const captions: SubtitleCue[] = parsed.cues.map((cue) => ({
		text: cue.text,
		startTime: cue.start,
		duration: cue.end - cue.start,
		...(cue.words && cue.words.length > 0 ? { words: cue.words } : {}),
	}));
	return { captions, skippedCueCount: parsed.skipped, warnings: [] };
}

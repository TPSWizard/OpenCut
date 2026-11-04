import type { TranscriptionSegment, TranscriptionWord } from "./types";

/** A pause between words long enough to start a new segment. */
export const SEGMENT_GAP_SECONDS = 1.0;
const SENTENCE_END = /[.!?。！？]["')\]]*$/;

/**
 * Groups timed words into sentence-like segments: a segment ends at
 * sentence punctuation or at a pause longer than `gapSeconds`.
 */
export function segmentsFromWords({
	words,
	gapSeconds = SEGMENT_GAP_SECONDS,
}: {
	words: readonly TranscriptionWord[];
	gapSeconds?: number;
}): TranscriptionSegment[] {
	const segments: TranscriptionSegment[] = [];
	let current: TranscriptionWord[] = [];

	const flush = () => {
		if (current.length === 0) return;
		segments.push({
			text: current.map((word) => word.text).join(" "),
			start: current[0].start,
			end: current[current.length - 1].end,
		});
		current = [];
	};

	for (const word of words) {
		const previous = current[current.length - 1];
		if (previous && word.start - previous.end > gapSeconds) {
			flush();
		}
		current.push(word);
		if (SENTENCE_END.test(word.text)) {
			flush();
		}
	}
	flush();
	return segments;
}

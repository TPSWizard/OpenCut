import type {
	TranscriptionSegment,
	TranscriptionWord,
	CaptionChunk,
} from "@/transcription/types";
import {
	DEFAULT_WORDS_PER_CAPTION,
	MAX_CAPTION_WORD_GAP_SECONDS,
	MIN_CAPTION_DURATION_SECONDS,
} from "@/transcription/caption-defaults";

const SENTENCE_END = /[.!?。！？]["')\]]*$/;

/**
 * Splits a transcript into caption chunks. With word timings each chunk
 * carries its words and starts and ends on real speech; a chunk closes
 * after `wordsPerChunk` words, at sentence punctuation, or at a pause
 * longer than `maxGap`. Without words the segments are cut evenly, as
 * before.
 */
export function buildCaptionChunks({
	segments,
	words,
	wordsPerChunk = DEFAULT_WORDS_PER_CAPTION,
	minDuration = MIN_CAPTION_DURATION_SECONDS,
	maxGap = MAX_CAPTION_WORD_GAP_SECONDS,
}: {
	segments: TranscriptionSegment[];
	words?: TranscriptionWord[];
	wordsPerChunk?: number;
	minDuration?: number;
	maxGap?: number;
}): CaptionChunk[] {
	if (words && words.length > 0) {
		return buildCaptionChunksFromWords({ words, wordsPerChunk, minDuration, maxGap });
	}
	return buildCaptionChunksFromSegments({ segments, wordsPerChunk, minDuration });
}

export function buildCaptionChunksFromWords({
	words,
	wordsPerChunk = DEFAULT_WORDS_PER_CAPTION,
	minDuration = MIN_CAPTION_DURATION_SECONDS,
	maxGap = MAX_CAPTION_WORD_GAP_SECONDS,
}: {
	words: readonly TranscriptionWord[];
	wordsPerChunk?: number;
	minDuration?: number;
	maxGap?: number;
}): CaptionChunk[] {
	const perChunk = Math.max(1, Math.floor(wordsPerChunk));
	const groups: TranscriptionWord[][] = [];
	let current: TranscriptionWord[] = [];

	for (const raw of words) {
		const text = raw.text.trim();
		if (!text) continue;
		const word = { text, start: raw.start, end: Math.max(raw.end, raw.start) };
		const previous = current[current.length - 1];
		if (previous && word.start - previous.end > maxGap) {
			groups.push(current);
			current = [];
		}
		current.push(word);
		if (current.length >= perChunk || SENTENCE_END.test(word.text)) {
			groups.push(current);
			current = [];
		}
	}
	if (current.length > 0) {
		groups.push(current);
	}

	const chunks: CaptionChunk[] = [];
	for (let index = 0; index < groups.length; index++) {
		const group = groups[index];
		const startTime = group[0].start;
		const spokenEnd = group[group.length - 1].end;
		// Hold short chunks on screen for a readable minimum, but never into
		// the next chunk.
		let end = Math.max(spokenEnd, startTime + minDuration);
		const next = groups[index + 1];
		if (next) {
			end = Math.min(end, next[0].start);
		}
		const duration = Math.max(end - startTime, spokenEnd - startTime, 0.01);
		chunks.push({
			text: group.map((word) => word.text).join(" "),
			startTime,
			duration,
			words: group,
		});
	}
	return chunks;
}

function buildCaptionChunksFromSegments({
	segments,
	wordsPerChunk,
	minDuration,
}: {
	segments: TranscriptionSegment[];
	wordsPerChunk: number;
	minDuration: number;
}): CaptionChunk[] {
	const captions: CaptionChunk[] = [];
	let globalEndTime = 0;

	for (const segment of segments) {
		const words = segment.text.trim().split(/\s+/);
		if (words.length === 0 || (words.length === 1 && words[0] === "")) continue;

		const segmentDuration = segment.end - segment.start;
		const wordsPerSecond = words.length / segmentDuration;

		const chunks: string[] = [];
		for (let i = 0; i < words.length; i += wordsPerChunk) {
			chunks.push(words.slice(i, i + wordsPerChunk).join(" "));
		}

		let chunkStartTime = segment.start;
		for (const chunk of chunks) {
			const chunkWords = chunk.split(/\s+/).length;
			const chunkDuration = Math.max(minDuration, chunkWords / wordsPerSecond);
			const adjustedStartTime = Math.max(chunkStartTime, globalEndTime);

			captions.push({
				text: chunk,
				startTime: adjustedStartTime,
				duration: chunkDuration,
			});

			globalEndTime = adjustedStartTime + chunkDuration;
			chunkStartTime += chunkDuration;
		}
	}

	return captions;
}

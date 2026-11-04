import { describe, expect, it } from "vitest";
import type { MediaTime } from "@/wasm";
import { getActiveWordState } from "../active-word";
import { placeWords } from "../word-layout";
import { highlightedWordIndices } from "@/text/primitives";
import { buildCaptionChunksFromWords } from "@/transcription/caption";
import { segmentsFromWords } from "@/transcription/segments";

const SECOND = 120_000;
const ticks = (seconds: number) => Math.round(seconds * SECOND) as MediaTime;

const words = [
	{ text: "Hello", start: ticks(0), end: ticks(0.4) },
	{ text: "there,", start: ticks(0.5), end: ticks(0.9) },
	{ text: "world", start: ticks(1.2), end: ticks(1.6) },
];

describe("getActiveWordState", () => {
	it("finds the word under the playhead and counts the words begun", () => {
		expect(getActiveWordState({ words, localTime: ticks(0.2) })).toEqual({
			activeIndex: 0,
			spokenCount: 1,
		});
		expect(getActiveWordState({ words, localTime: ticks(1.3) })).toEqual({
			activeIndex: 2,
			spokenCount: 3,
		});
	});

	it("has no active word in a pause but keeps the spoken count", () => {
		expect(getActiveWordState({ words, localTime: ticks(1.0) })).toEqual({
			activeIndex: -1,
			spokenCount: 2,
		});
		expect(getActiveWordState({ words, localTime: ticks(5) })).toEqual({
			activeIndex: -1,
			spokenCount: 3,
		});
	});

	it("has nothing before the first word", () => {
		const late = words.map((word) => ({
			...word,
			start: ticks(1 + word.start / SECOND) as MediaTime,
			end: ticks(1 + word.end / SECOND) as MediaTime,
		}));
		expect(getActiveWordState({ words: late, localTime: ticks(0.5) })).toEqual({
			activeIndex: -1,
			spokenCount: 0,
		});
	});
});

describe("placeWords", () => {
	it("numbers the tokens across wrapped lines with their prefixes", () => {
		expect(placeWords({ lines: ["Hello there,", "  world"] })).toEqual([
			{ index: 0, line: 0, prefix: "", text: "Hello" },
			{ index: 1, line: 0, prefix: "Hello ", text: "there," },
			{ index: 2, line: 1, prefix: "  ", text: "world" },
		]);
	});
});

describe("highlightedWordIndices", () => {
	const base = { activeColor: "#fff", boxColor: "#000", activeIndex: 1, spokenCount: 2 };
	it("singles out the active word, or everything spoken for fill", () => {
		expect(highlightedWordIndices({ highlight: { ...base, mode: "word" } })).toEqual([1]);
		expect(highlightedWordIndices({ highlight: { ...base, mode: "box" } })).toEqual([1]);
		expect(highlightedWordIndices({ highlight: { ...base, mode: "fill" } })).toEqual([0, 1]);
		expect(highlightedWordIndices({ highlight: { ...base, mode: "none" } })).toEqual([]);
		expect(
			highlightedWordIndices({ highlight: { ...base, mode: "word", activeIndex: -1 } }),
		).toEqual([]);
	});
});

describe("buildCaptionChunksFromWords", () => {
	const spoken = [
		{ text: " The", start: 0.0, end: 0.2 },
		{ text: " quick", start: 0.2, end: 0.5 },
		{ text: " brown", start: 0.5, end: 0.8 },
		{ text: " fox.", start: 0.8, end: 1.1 },
		{ text: " It", start: 3.0, end: 3.2 },
		{ text: " jumps", start: 3.2, end: 3.6 },
		{ text: " over", start: 3.6, end: 3.8 },
		{ text: " the", start: 5.5, end: 5.6 },
		{ text: " dog", start: 5.6, end: 6.0 },
	];

	it("cuts after N words, at sentence ends and at long pauses, keeping the words", () => {
		const chunks = buildCaptionChunksFromWords({ words: spoken, wordsPerChunk: 3 });
		expect(chunks.map((chunk) => chunk.text)).toEqual([
			"The quick brown",
			"fox.",
			"It jumps over",
			"the dog",
		]);
		expect(chunks[0].startTime).toBe(0);
		expect(chunks[0].words?.map((word) => word.text)).toEqual(["The", "quick", "brown"]);
		// The chunk's words spell its text.
		for (const chunk of chunks) {
			expect(chunk.words?.map((word) => word.text).join(" ")).toBe(chunk.text);
		}
	});

	it("holds short chunks for the minimum duration without reaching the next", () => {
		const chunks = buildCaptionChunksFromWords({
			words: spoken,
			wordsPerChunk: 3,
			minDuration: 0.8,
		});
		// "fox." is 0.3 s of speech, held to 0.8 s (the next chunk starts at 3 s).
		expect(chunks[1].startTime).toBeCloseTo(0.8);
		expect(chunks[1].duration).toBeCloseTo(0.8);
		// "The quick brown" ends at 0.8 s where "fox." starts: no overlap.
		expect(chunks[0].startTime + chunks[0].duration).toBeCloseTo(0.8);
		// The last chunk keeps its spoken length when that is longer.
		expect(chunks[3].duration).toBeCloseTo(0.8);
	});

	it("never ends a chunk before its last word is spoken", () => {
		const chunks = buildCaptionChunksFromWords({
			words: [
				{ text: "a", start: 0, end: 2 },
				{ text: "b", start: 2.1, end: 2.2 },
			],
			wordsPerChunk: 1,
			minDuration: 0.5,
		});
		expect(chunks[0].duration).toBeCloseTo(2);
	});
});

describe("segmentsFromWords", () => {
	it("groups words into sentences and at pauses", () => {
		const segments = segmentsFromWords({
			words: [
				{ text: "One", start: 0, end: 0.3 },
				{ text: "two.", start: 0.3, end: 0.6 },
				{ text: "Three", start: 0.7, end: 1.0 },
				{ text: "four", start: 3.0, end: 3.3 },
			],
		});
		expect(segments).toEqual([
			{ text: "One two.", start: 0, end: 0.6 },
			{ text: "Three", start: 0.7, end: 1.0 },
			{ text: "four", start: 3.0, end: 3.3 },
		]);
	});
});

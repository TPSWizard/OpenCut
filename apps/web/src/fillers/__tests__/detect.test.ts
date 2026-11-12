import { describe, expect, it } from "vitest";
import { fillerIntervals, fillerSet, findFillers, normalizeWord, totalSeconds } from "../detect";

const words = [
	{ text: "So,", start: 0.0, end: 0.3 },
	{ text: "um,", start: 0.4, end: 0.7 },
	{ text: "the", start: 0.75, end: 0.9 },
	{ text: "plan", start: 0.9, end: 1.3 },
	{ text: "is", start: 1.3, end: 1.45 },
	{ text: "uhhh", start: 1.5, end: 1.9 },
	{ text: "Umm", start: 1.95, end: 2.2 },
	{ text: "simple.", start: 2.3, end: 2.8 },
	{ text: "Hmm", start: 4.0, end: 6.0 },
];

describe("normalizeWord", () => {
	it("lower-cases, drops punctuation and collapses letter runs", () => {
		expect(normalizeWord({ text: "Ummm," })).toBe("um");
		expect(normalizeWord({ text: "Ähm…" })).toBe("ähm");
		expect(normalizeWord({ text: "you'll" })).toBe("youl");
		expect(normalizeWord({ text: "123" })).toBe("");
	});
});

describe("fillerSet", () => {
	it("knows the languages and keeps real words out of the automatic set", () => {
		expect(fillerSet({ language: "en" }).has("um")).toBe(true);
		expect(fillerSet({ language: "fr" }).has("ben")).toBe(true);
		expect(fillerSet({ language: "auto" }).has("ben")).toBe(false);
		expect(fillerSet({ language: "auto" }).has("ähm")).toBe(true);
		expect(fillerSet({ language: "xx-YY" }).has("um")).toBe(true);
	});
});

describe("findFillers and fillerIntervals", () => {
	it("pads fillers without touching neighbours, merges runs and skips long matches", () => {
		const fillers = findFillers({ words, language: "en" });
		expect(fillers.map((hit) => hit.filler)).toEqual(["um", "uh", "um"]);
		const intervals = fillerIntervals({ words, fillers });
		expect(intervals).toHaveLength(2);
		// "um" at 0.4–0.7: the padding fits on both sides (0.36 > 0.3, 0.74 < 0.75).
		expect(intervals[0].start).toBeCloseTo(0.36, 9);
		expect(intervals[0].end).toBeCloseTo(0.74, 9);
		// "uhhh" and "Umm" follow each other and merge; the end stays short of "simple." (2.24 < 2.3).
		expect(intervals[1].start).toBeCloseTo(1.46, 9);
		expect(intervals[1].end).toBeCloseTo(2.24, 9);
		expect(totalSeconds({ intervals })).toBeCloseTo(0.38 + 0.78, 6);
	});

	it("finds nothing in clean speech", () => {
		const clean = [
			{ text: "The", start: 0, end: 0.2 },
			{ text: "quick", start: 0.2, end: 0.5 },
		];
		expect(findFillers({ words: clean, language: "auto" })).toEqual([]);
		expect(fillerIntervals({ words: clean, fillers: [] })).toEqual([]);
	});
});

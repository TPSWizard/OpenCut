import { describe, expect, it } from "vitest";
import type { SceneTracks, TextElement } from "@/timeline/types";
import type { MediaTime } from "@/wasm";
import { retimeCaptionTracks, transcriptCuts, type TranscriptWord } from "../cuts";

const word = (text: string, start: number, end: number): TranscriptWord => ({ text, start, end });
const sentence = [word("So", 1.0, 1.2), word("the", 1.4, 1.5), word("plan", 1.5, 1.9), word("is", 2.1, 2.3), word("simple.", 2.5, 3.0)];
const close = (a: number, b: number) => Math.abs(a - b) < 1e-6;

// The same cases as crates/editor/src/transcript.rs.
describe("transcriptCuts", () => {
	it("cuts from pause middle to pause middle", () => {
		const cuts = transcriptCuts({ words: sentence, deleted: [1, 2], from: 0, to: 10 });
		expect(cuts).toHaveLength(1);
		expect(close(cuts[0].start, 1.3) && close(cuts[0].end, 2.0)).toBe(true);
		expect(transcriptCuts({ words: sentence, deleted: [1, 3, 99], from: 0, to: 10 })).toHaveLength(2);
		expect(transcriptCuts({ words: sentence, deleted: [], from: 0, to: 10 })).toEqual([]);
	});

	it("pads the ends and stays inside the clip", () => {
		let cuts = transcriptCuts({ words: sentence, deleted: [0], from: 0.95, to: 10 });
		expect(close(cuts[0].start, 0.95) && close(cuts[0].end, 1.3)).toBe(true);
		cuts = transcriptCuts({ words: sentence, deleted: [4], from: 0, to: 10 });
		expect(close(cuts[0].start, 2.4) && close(cuts[0].end, 3.08)).toBe(true);
		expect(transcriptCuts({ words: sentence, deleted: [0, 1, 2, 3, 4], from: 0, to: 10 })).toHaveLength(1);
		cuts = transcriptCuts({ words: [...sentence].reverse(), deleted: [2, 3], from: 0, to: 10 });
		expect(close(cuts[0].start, 1.3) && close(cuts[0].end, 2.0)).toBe(true);
	});
});

describe("retimeCaptionTracks", () => {
	const T = 120_000;
	const ticks = (seconds: number) => Math.round(seconds * T) as MediaTime;
	const caption = (id: string, words: TranscriptWord[]): TextElement => {
		const start = words[0].start;
		return {
			id,
			type: "text",
			name: id,
			startTime: ticks(start),
			duration: ticks(words[words.length - 1].end - start),
			trimStart: 0 as MediaTime,
			trimEnd: 0 as MediaTime,
			params: { content: words.map((w) => w.text).join(" ") },
			words: words.map((w) => ({ text: w.text, start: ticks(w.start - start), end: ticks(w.end - start) })),
		} as unknown as TextElement;
	};
	const all = [...sentence, word("Later", 5.0, 5.4), word("words.", 5.5, 6.0)];
	const tracks = {
		overlay: [{ id: "captions", type: "text", elements: [caption("a", all.slice(0, 3)), caption("b", all.slice(3, 5)), caption("c", all.slice(5))] }],
		main: { id: "main", type: "video", elements: [] },
		audio: [],
	} as unknown as SceneTracks;

	it("drops struck words and moves later ones with the ripple", () => {
		// "plan is" struck: the cut is 1.5..2.4.
		const [cut] = transcriptCuts({ words: all, deleted: [2, 3], from: 0, to: 10 });
		const result = retimeCaptionTracks({ tracks, ranges: [{ start: ticks(cut.start), end: ticks(cut.end) }], ripple: "all" });
		const [a, b, c] = result.overlay[0].elements as TextElement[];
		expect(a.params.content).toBe("So the");
		expect(close(a.duration / T, 0.5)).toBe(true);
		expect(b.params.content).toBe("simple.");
		expect(close(b.startTime / T, 1.5) && close(b.duration / T, 0.6)).toBe(true);
		expect(close(b.words?.[0].start ?? -1, ticks(0.1))).toBe(true);
		// Starts after the cut: the ripple itself moves it, so it is untouched here.
		expect(c).toBe(tracks.overlay[0].elements[2]);
	});

	it("leaves timings alone without a ripple and removes emptied captions", () => {
		const result = retimeCaptionTracks({ tracks, ranges: [{ start: ticks(4.9), end: ticks(6.1) }], ripple: "none" });
		expect(result.overlay[0].elements.map((element) => element.id)).toEqual(["a", "b"]);
		const kept = retimeCaptionTracks({ tracks, ranges: [{ start: ticks(1.45), end: ticks(1.95) }], ripple: "track" });
		const a = kept.overlay[0].elements[0] as TextElement;
		expect(a.params.content).toBe("So");
		expect(a.startTime).toBe(ticks(1.0));
		expect(a.duration).toBe(ticks(0.45));
	});
});

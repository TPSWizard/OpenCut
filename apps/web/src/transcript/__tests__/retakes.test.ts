import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { transcriptCuts } from "../cuts";
import { retakeWords, transcriptRetakes, type RetakeOptions } from "../retakes";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/retakes/cases.json"), "utf8")) as {
	cases: {
		name: string;
		options: RetakeOptions;
		words: { text: string; start: number; end: number }[];
		retakes: { kind: string; words: number[]; kept: number; similarity: number }[];
		cut: number[];
	}[];
};

describe("retakes through WASM", () => {
	it("finds the same retakes as the reference", () => {
		for (const c of fixture.cases) {
			const got = transcriptRetakes(c.words, c.options);
			expect(got.map((r) => ({ kind: r.kind, words: r.words, kept: r.kept, similarity: r.similarity })), c.name).toEqual(c.retakes);
			expect(retakeWords(got), c.name).toEqual(c.cut);
		}
	});

	it("cuts a false start up to the kept take, as on the desktop", () => {
		// The editor crate's test: "So the plan is... um, So the plan is to launch." at 10 s.
		const texts = ["So", "the", "plan", "is...", "um,", "So", "the", "plan", "is", "to", "launch."];
		const words = texts.map((text, index) => ({ text, start: 10 + index * 0.4, end: 10.3 + index * 0.4 }));
		const found = transcriptRetakes(words);
		expect(found).toHaveLength(1);
		expect(found[0]).toMatchObject({ kind: "retake", kept: 5, text: "So the plan is... um,", keptText: "So the plan is to launch." });
		const cuts = transcriptCuts({ words, deleted: retakeWords(found), from: 9, to: 20 });
		expect(cuts).toHaveLength(1);
		expect(cuts[0].start).toBeCloseTo(9.92, 9);
		expect(cuts[0].end).toBeCloseTo(11.95, 9);
	});

	it("uses the defaults when no options are given", () => {
		expect(transcriptRetakes([])).toEqual([]);
		const words = [
			{ text: "We", start: 0, end: 0.2 },
			{ text: "ship", start: 0.3, end: 0.5 },
			{ text: "today.", start: 0.6, end: 0.9 },
		];
		expect(transcriptRetakes(words)).toEqual([]);
	});
});

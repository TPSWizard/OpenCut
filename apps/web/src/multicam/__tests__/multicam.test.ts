import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { multicamAddCut, multicamCutCount, multicamOnsets, multicamOpacityKeys, multicamSyncLag } from "opencut-wasm";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/multicam/cases.json"), "utf8")) as {
	sync: { name: string; rate: number; maxLag: number; reference: number[]; other: number[]; referenceOnsetsHead: number[]; lag: number; score: number; trueLag: number }[];
	program: { name: string; cuts: [number, number][]; clips: { angle: number; start: number; end: number; keys: [number, number][] }[] }[];
	cutLists: { text: string; parsed: [number, number][]; addTime: number; addAngle: number; afterAdd: string }[];
};

const encode = (cuts: [number, number][]) => cuts.map(([time, angle]) => `${time.toFixed(3)}:${angle}`).join(",");

describe("multicam through WASM", () => {
	it.each(cases.sync.map((c) => [c.name, c] as const))("syncs: %s", (_, c) => {
		const a = multicamOnsets(new Float32Array(c.reference), c.rate);
		const b = multicamOnsets(new Float32Array(c.other), c.rate);
		c.referenceOnsetsHead.forEach((value, index) => expect(a[index]).toBeCloseTo(value, 5));
		const [lag, score] = multicamSyncLag(a, b, c.maxLag);
		expect(lag).toBeCloseTo(c.lag, 6);
		expect(score).toBeCloseTo(c.score, 6);
		expect(Math.abs(lag - c.trueLag)).toBeLessThan(0.01);
	});

	it.each(cases.program.map((c) => [c.name, c] as const))("steps opacity: %s", (_, c) => {
		const text = encode(c.cuts);
		for (const clip of c.clips) {
			const flat = multicamOpacityKeys(text, clip.angle, clip.start, clip.end);
			const keys: [number, number][] = [];
			for (let index = 0; index + 1 < flat.length; index += 2) keys.push([flat[index], flat[index + 1]]);
			expect(keys.length).toBe(clip.keys.length);
			keys.forEach(([time, value], index) => {
				expect(time).toBeCloseTo(clip.keys[index][0], 9);
				expect(value).toBe(clip.keys[index][1]);
			});
		}
	});

	it.each(cases.cutLists.map((c) => [c.text || "(empty)", c] as const))("cut list: %s", (_, c) => {
		expect(multicamCutCount(c.text)).toBe(c.parsed.length);
		expect(multicamAddCut(c.text, c.addTime, c.addAngle)).toBe(c.afterAdd);
	});
});

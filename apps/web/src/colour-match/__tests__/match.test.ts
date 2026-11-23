import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { decodeLutParam, sampleLut } from "@/luts/parse";
import {
	COLOUR_MATCH_LUT_SIZE,
	ColourAccumulator,
	colourMatchLut,
	colourMatchParam,
	isColourMatchParam,
	labToRgb,
	rgbToLab,
	transferColour,
} from "../match";

const root = resolve(__dirname, "../../../../..");

type Triple = [number, number, number];
interface Cases {
	lab: { rgb: Triple; lab: Triple }[];
	stats: { name: string; pixels: number[][]; count: number; mean: Triple; std: Triple }[];
	transfer: { name: string; source: { mean: Triple; std: Triple }; reference: { mean: Triple; std: Triple }; samples: { rgb: Triple; out: Triple }[] }[];
}

const cases = JSON.parse(readFileSync(resolve(root, "fixtures/colour-match/cases.json"), "utf8")) as Cases;

const close = (actual: readonly number[], expected: readonly number[], label: string, digits = 6) =>
	expected.forEach((value, index) => expect(actual[index], `${label} [${index}]`).toBeCloseTo(value, digits));

describe("colour match", () => {
	it("converts to Lab like the shared cases", () => {
		for (const { rgb, lab } of cases.lab) {
			close(rgbToLab(...rgb), lab, `lab ${rgb}`);
			close(labToRgb(...lab), rgb, `round trip ${rgb}`, 5);
		}
	});

	it("measures pixels like the shared cases", () => {
		for (const { name, pixels, count, mean, std } of cases.stats) {
			const accumulator = new ColourAccumulator();
			accumulator.addRgba(pixels.flat());
			const stats = accumulator.stats()!;
			expect(stats.count, name).toBe(count);
			close(stats.mean, mean, `${name} mean`);
			close(stats.std, std, `${name} std`);
		}
		expect(new ColourAccumulator().stats()).toBeNull();
	});

	it("moves colours like the shared cases", () => {
		for (const { name, source, reference, samples } of cases.transfer) {
			for (const { rgb, out } of samples) close(transferColour(rgb, source, reference), out, `${name} ${rgb}`);
		}
	});

	it("bakes the transfer into a lut effect parameter", () => {
		const { source, reference, samples } = cases.transfer[0];
		const table = colourMatchLut(source, reference);
		expect(table.size).toBe(COLOUR_MATCH_LUT_SIZE);
		for (const { rgb, out } of samples) close(sampleLut({ table, r: rgb[0], g: rgb[1], b: rgb[2] }), out, `grid ${rgb}`, 1);
		const param = colourMatchParam(source, reference);
		expect(isColourMatchParam(param)).toBe(true);
		expect(isColourMatchParam("Film|2|AAAA")).toBe(false);
		const decoded = decodeLutParam({ param })!;
		expect(decoded.name).toBe("Colour match");
		expect(decoded.table.size).toBe(COLOUR_MATCH_LUT_SIZE);
	});

	it("leaves colours alone when the statistics already match", () => {
		const same = cases.transfer.find((item) => item.name === "same stats is identity")!;
		for (const { rgb, out } of same.samples) close(out, rgb, `identity ${rgb}`, 5);
	});
});

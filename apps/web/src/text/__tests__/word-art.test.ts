import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { wordArtCompose } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { WORD_ART_KEYS, getWordArtStyles, wordArtOutsetPx, wordArtParamsJson, wordArtStyleParamsFor } from "../word-art";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/word-art/cases.json"), "utf8")) as Array<{
	name: string;
	width: number;
	height: number;
	em: number;
	params: Record<string, unknown>;
	input: number[];
	output: number[];
}>;

describe("word art", () => {
	it("styles glyphs as the reference does, through WASM", () => {
		expect(cases.length).toBeGreaterThanOrEqual(8);
		for (const item of cases) {
			const json = wordArtParamsJson(item.params);
			expect(json, item.name).not.toBeNull();
			const got = wordArtCompose(json as string, item.width, item.height, new Uint8Array(item.input), item.em);
			expect(got.length, item.name).toBe(item.output.length);
			let worst = 0;
			for (let i = 0; i < got.length; i++) worst = Math.max(worst, Math.abs(got[i] - item.output[i]));
			expect(worst, item.name).toBeLessThanOrEqual(1);
		}
	});

	it("skips plain text and sizes the outlines", () => {
		expect(wordArtParamsJson({ color: "#fff" })).toBeNull();
		expect(wordArtParamsJson({ "wordArt.fill": "solid", "wordArt.stroke1Width": 0 })).toBeNull();
		const json = wordArtParamsJson({ "wordArt.stroke1Width": 10, "wordArt.stroke3Width": 5, color: "#fff" });
		expect(JSON.parse(json as string)).toEqual({ "wordArt.stroke1Width": 10, "wordArt.stroke3Width": 5 });
		// 15% of a 100 px em, plus a pixel.
		expect(wordArtOutsetPx(json, 100)).toBe(16);
		expect(wordArtOutsetPx(null, 100)).toBe(0);
	});

	it("has a gallery whose styles set every word-art param", () => {
		const styles = getWordArtStyles();
		expect(styles[0].id).toBe("none");
		expect(styles.length).toBeGreaterThanOrEqual(10);
		for (const style of styles) {
			const params = wordArtStyleParamsFor(style.id);
			expect(params, style.id).not.toBeNull();
			for (const key of WORD_ART_KEYS) expect(params, `${style.id} ${key}`).toHaveProperty([key]);
			expect(params?.["wordArt.style"]).toBe(style.id);
			expect(wordArtParamsJson(params as Record<string, unknown>) === null).toBe(style.id === "none");
		}
		expect(wordArtStyleParamsFor("nope")).toBeNull();
	});
});

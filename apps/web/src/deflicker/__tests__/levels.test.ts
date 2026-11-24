import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DeflickerLevels, deflickerEncodeLevels, deflickerFrameLevel } from "opencut-wasm";
import { deflickerGainAt, deflickerParams, readDeflickerLevels, readDeflickerStrength, strengthWindow } from "../levels";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/deflicker/cases.json"), "utf8")) as {
	levels: { width: number; height: number; rgba: number[]; level: number }[];
	gains: { name: string; levels: number[]; window: number; gains: number[] }[];
	lookups: {
		text: string;
		valid?: boolean;
		mediaId?: string;
		encoded?: string;
		probes?: { time: number; window: number; index: number; gain: number }[];
	}[];
};

describe("deflicker through WASM", () => {
	it("measures frames like the reference", () => {
		for (const c of cases.levels) {
			expect(deflickerFrameLevel(new Uint8Array(c.rgba), c.width, c.height)).toBeCloseTo(c.level, 9);
		}
	});

	it("gives each frame the reference gain", () => {
		for (const c of cases.gains) {
			const levels = new DeflickerLevels(deflickerEncodeLevels("m", 0, 1, new Float64Array(c.levels)));
			c.gains.forEach((gain, index) => expect(levels.gainAt(index, c.window)).toBeCloseTo(gain, 2));
			levels.free();
		}
	});

	it("reads stored levels like the reference", () => {
		for (const c of cases.lookups) {
			if (c.valid === false) {
				expect(() => new DeflickerLevels(c.text)).toThrow();
				continue;
			}
			const levels = new DeflickerLevels(c.text);
			expect(levels.mediaId()).toBe(c.mediaId);
			for (const probe of c.probes ?? []) expect(levels.gainAt(probe.time, probe.window)).toBeCloseTo(probe.gain, 9);
			levels.free();
		}
	});
});

describe("deflicker params", () => {
	const text = cases.lookups[0].text;

	it("needs a strength and levels measured from this media", () => {
		expect(readDeflickerStrength({})).toBe("off");
		expect(readDeflickerStrength({ "deflicker.strength": "nope" })).toBe("off");
		expect(strengthWindow("light")).toBe(5);
		expect(strengthWindow("medium")).toBe(11);
		expect(strengthWindow("strong")).toBe(25);
		expect(deflickerParams({ "deflicker.levels": text }, "media-1")).toBeNull();
		expect(deflickerParams({ "deflicker.levels": text, "deflicker.strength": "light" }, "other")).toBeNull();
		expect(readDeflickerLevels({ "deflicker.levels": "junk" }, "media-1")).toBeNull();
		const on = deflickerParams({ "deflicker.levels": text, "deflicker.strength": "light" }, "media-1");
		expect(on).toEqual({ window: 5, levels: text });
		const probe = cases.lookups[0].probes?.[1];
		if (!on || !probe) throw new Error("fixture");
		const gain = deflickerGainAt({ deflicker: on, sourceSeconds: probe.time });
		expect(gain).toBeCloseTo(probe.gain, 9);
	});
});

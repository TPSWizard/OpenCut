import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
	COLOUR_WHEEL_RANGE,
	HSL_BANDS,
	colourWheelMove,
	colourWheelOffsets,
	colourWheelPoint,
	colourWheelRgba,
	hslBandValue,
	hslParamKey,
} from "../colour-tools";
import { hslEffectDefinition } from "./library";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/hsl/cases.json"), "utf8")) as {
	bands: string[];
	curve: { values: number[]; points: { hue: number; value: number }[] };
	wheelRange: number;
	wheels: { point: [number, number] | null; offsets: number[]; back: [number, number] }[];
};

describe("HSL effect", () => {
	it("fills the shared shader's 24 uniforms, clamped", () => {
		const pass = hslEffectDefinition.renderer.buildPasses?.({
			effectParams: { redHue: 40, aquaSaturation: -100, magentaLightness: 250 },
			width: 1920,
			height: 1080,
			timeSeconds: 0,
		})[0];
		expect(pass?.shader).toBe("hsl");
		expect(Object.keys(pass?.uniforms ?? {})).toHaveLength(24);
		expect(pass?.uniforms).toMatchObject({ u_hue_red: 40, u_sat_aqua: -100, u_light_magenta: 100, u_hue_blue: 0 });
	});

	it("matches the desktop catalogue's keys, labels and defaults", () => {
		const rust = readFileSync(resolve(root, "crates/editor/src/effect_catalogue.rs"), "utf8");
		const block = rust.slice(rust.indexOf('kind: "hsl"'), rust.indexOf('kind: "lut"'));
		expect(hslEffectDefinition.params).toHaveLength(24);
		for (const param of hslEffectDefinition.params) {
			expect(block).toContain(`number("${param.key}", "${param.label}", 0.0, -100.0, 100.0, 1.0)`);
		}
		expect(fixture.bands).toEqual(HSL_BANDS.map((band) => band.key));
		expect(hslParamKey(4, 2)).toBe("aquaLightness");
	});

	it("draws the hue curves and colour wheels as the reference (and the desktop)", () => {
		for (const point of fixture.curve.points) {
			expect(hslBandValue(fixture.curve.values, point.hue)).toBeCloseTo(point.value, 9);
		}
		expect(COLOUR_WHEEL_RANGE).toBe(fixture.wheelRange);
		for (const wheel of fixture.wheels) {
			if (wheel.point) {
				const got = colourWheelOffsets(wheel.point[0], wheel.point[1]);
				got.forEach((value, channel) => expect(value).toBeCloseTo(wheel.offsets[channel], 12));
			}
			const back = colourWheelPoint(wheel.offsets);
			expect(back.x).toBeCloseTo(wheel.back[0], 12);
			expect(back.y).toBeCloseTo(wheel.back[1], 12);
		}
		for (const value of colourWheelMove([0.3, 0.1, 0.1], 0, 0)) expect(value).toBeCloseTo(0.5 / 3, 12);
		const picture = colourWheelRgba(32);
		expect(picture[3]).toBe(0);
	});
});

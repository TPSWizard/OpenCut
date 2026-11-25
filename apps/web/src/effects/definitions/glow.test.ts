import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { LIGHT_LEAK_COLOURS, SOURCE_TABLE, bloomEffectDefinition, lightLeakEffectDefinition } from "./library";

const rust = readFileSync(resolve(__dirname, "../../../../../crates/editor/src/effect_catalogue.rs"), "utf8");
const build = (definition: typeof bloomEffectDefinition, params: Record<string, unknown>, height = 1080, timeSeconds = 0) =>
	definition.renderer.buildPasses?.({ effectParams: params as never, width: (height * 16) / 9, height, timeSeconds }) ?? [];

describe("bloom", () => {
	it("keeps the highlights, blurs them and adds them back onto the untouched frame", () => {
		const passes = build(bloomEffectDefinition, { threshold: 0.6, intensity: 1.2, radius: 40, softness: 0.1, tint: "#ff8000" });
		expect(passes[0]).toEqual({ shader: "bloom-extract", uniforms: { u_threshold: 0.6, u_knee: 0.1 } });
		const blurs = passes.slice(1, -1);
		expect(blurs.length).toBeGreaterThanOrEqual(2);
		expect(blurs.every((pass) => pass.shader === "gaussian-blur")).toBe(true);
		expect(blurs[0].uniforms.u_sigma).toBeCloseTo(20, 9);
		const combine = passes[passes.length - 1];
		expect(combine.shader).toBe("bloom-combine");
		expect(combine.uniforms).toEqual({ u_intensity: 1.2, u_tint_r: 1, u_tint_g: 128 / 255, u_tint_b: 0 });
		expect(combine.lut?.key).toBe(SOURCE_TABLE);
		// The radius is given at 1080 high and scales with the frame, as on the desktop.
		expect(build(bloomEffectDefinition, { radius: 40 }, 540)[1].uniforms.u_sigma).toBeCloseTo(10, 9);
		// A tint that does not read is white.
		expect(build(bloomEffectDefinition, { tint: "nope" }).at(-1)?.uniforms).toMatchObject({ u_tint_r: 1, u_tint_g: 1, u_tint_b: 1 });
	});
});

describe("light leak", () => {
	it("moves with the clip's time and takes its colours from the shared table", () => {
		const [pass] = build(lightLeakEffectDefinition, { colours: "cool", intensity: 0.4, angle: 90, speed: 1.5 }, 1080, 2.5);
		expect(pass.shader).toBe("light-leak");
		expect(pass.uniforms).toMatchObject({ u_intensity: 0.4, u_time: 2.5, u_speed: 1.5, u_colour1_r: 0.35, u_colour2_r: 0.63 });
		expect(pass.uniforms.u_angle).toBeCloseTo(Math.PI / 2, 12);
		expect(Object.keys(pass.uniforms)).toHaveLength(12);
	});
});

describe("catalogue", () => {
	it("matches the desktop's keys, labels, defaults and leak colours", () => {
		for (const [definition, from, to] of [
			[bloomEffectDefinition, 'kind: "bloom"', 'kind: "light-leak"'],
			[lightLeakEffectDefinition, 'kind: "light-leak"', 'kind: "lut"'],
		] as const) {
			const block = rust.slice(rust.indexOf(from), rust.indexOf(to));
			for (const param of definition.params) {
				if (param.type === "number") {
					const text = Number.isInteger(param.default) ? param.default.toFixed(1) : String(param.default);
					expect(block).toContain(`number("${param.key}", "${param.label}", ${text}, `);
				} else if (param.type === "select") {
					for (const option of param.options) expect(block).toContain(`("${option.value}", "${option.label}")`);
				} else {
					expect(block).toContain(`color("${param.key}", "${param.label}", "${param.default}")`);
				}
			}
		}
		const fmt = (value: number) => (Number.isInteger(value) ? value.toFixed(1) : String(value));
		for (const [name, [first, second]] of Object.entries(LIGHT_LEAK_COLOURS)) {
			expect(rust).toContain(`("${name}", [${first.map(fmt).join(", ")}], [${second.map(fmt).join(", ")}])`);
		}
	});
});

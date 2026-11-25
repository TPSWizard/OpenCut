import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { withFaceOvals } from "@/retouch/track";
import { faceBlurEffectDefinition } from "./library";

const passesFor = (params: Record<string, number | string>) =>
	faceBlurEffectDefinition.renderer.buildPasses?.({
		effectParams: params as Record<string, number>,
		width: 1920,
		height: 1080,
		timeSeconds: 0,
	}) ?? [];

describe("face blur", () => {
	it("blurs in two passes, across then down, on faces only", () => {
		const passes = passesFor({});
		expect(passes).toHaveLength(2);
		expect(passes.map((pass) => pass.uniforms.u_direction)).toEqual([
			[1, 0],
			[0, 1],
		]);
		for (const pass of passes) {
			expect(pass.shader).toBe("face-blur");
			expect(pass.faces).toBe(true);
			expect(pass.uniforms).toMatchObject({ u_style: 0, u_strength: 1, u_size: 1.2, u_feather: 0.2, u_face0_rx: 0 });
		}
	});

	it("pixelates or blacks out in one pass", () => {
		const pixelate = passesFor({ style: "pixelate", strength: 1.5 });
		expect(pixelate).toHaveLength(1);
		expect(pixelate[0].uniforms).toMatchObject({ u_style: 1, u_strength: 1.5 });
		expect(pixelate[0].uniforms).not.toHaveProperty("u_direction");
		expect(passesFor({ style: "black" })[0].uniforms.u_style).toBe(2);
		// Out-of-range values are held to what the shader reads.
		expect(passesFor({ strength: 9, size: 0, feather: 3 })[0].uniforms).toMatchObject({ u_strength: 2, u_size: 0.1, u_feather: 1 });
	});

	it("gets the layer's face ovals, and draws nothing without faces", () => {
		const transform = { centerX: 960, centerY: 540, width: 1920, height: 1080, rotationDegrees: 0, flipX: false, flipY: false };
		const filled = withFaceOvals([passesFor({})], [0.5, 0.4, 0.2, 0.3], transform);
		expect(filled[0]).toHaveLength(2);
		for (const pass of filled[0]) {
			expect(pass.uniforms.u_face0_rx).toBeCloseTo(0.6 * 0.2 * 1920, 9);
			expect(pass.uniforms.u_direction).toBeDefined();
		}
		expect(withFaceOvals([passesFor({})], null, transform)).toEqual([]);
	});

	it("matches the desktop catalogue's keys, labels and defaults", () => {
		const rust = readFileSync(resolve(__dirname, "../../../../../crates/editor/src/effect_catalogue.rs"), "utf8");
		const block = rust.slice(rust.indexOf('kind: "face-blur"'), rust.indexOf('kind: "chroma-key"'));
		expect(block).toContain('label: "Face blur"');
		for (const param of faceBlurEffectDefinition.params) {
			if (param.type === "number") {
				const rust = (value: number | undefined) => (value ?? 0).toFixed(Math.max(1, String(value ?? 0).split(".")[1]?.length ?? 0));
				expect(block).toContain(`number("${param.key}", "${param.label}", ${rust(param.default)}, ${rust(param.min)}, ${rust(param.max)}, `);
			} else if (param.type === "select") {
				expect(block).toContain(`key: "${param.key}"`);
				for (const option of param.options) expect(block).toContain(`("${option.value}", "${option.label}")`);
			}
		}
	});
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { chromaKeyEffectDefinition, keyColorChannels } from "./library";

const passFor = (params: Record<string, string | number>) =>
	chromaKeyEffectDefinition.renderer.buildPasses?.({ effectParams: params, width: 1920, height: 1080, timeSeconds: 0 })[0];

describe("chroma key", () => {
	it("reads the key colour from hex", () => {
		expect(keyColorChannels("#0047bb")).toEqual([0, 0x47 / 255, 0xbb / 255]);
		expect(keyColorChannels("0f0")).toEqual([0, 1, 0]);
		// Unreadable falls back to the default green, as the desktop does.
		expect(keyColorChannels("nonsense")).toEqual([0, 0xb1 / 255, 0x40 / 255]);
	});

	it("fills the shader's uniforms", () => {
		const pass = passFor({ keyColor: "#0047bb", strength: 0.3, softness: 0.2, spill: 1 });
		expect(pass?.shader).toBe("chroma-key");
		expect(pass?.uniforms).toEqual({
			u_key_r: 0,
			u_key_g: 0x47 / 255,
			u_key_b: 0xbb / 255,
			u_strength: 0.3,
			u_softness: 0.2,
			u_spill: 1,
		});
		expect(passFor({})?.uniforms.u_strength).toBe(0.4);
	});

	it("matches the desktop catalogue's keys and defaults", () => {
		const rust = readFileSync(resolve(__dirname, "../../../../../crates/editor/src/effect_catalogue.rs"), "utf8");
		const block = rust.slice(rust.indexOf('kind: "chroma-key"'), rust.indexOf('kind: "shadows-highlights"'));
		expect(block).toContain('color("keyColor", "Key colour", "#00b140")');
		for (const param of chromaKeyEffectDefinition.params) {
			if (param.type === "number") {
				expect(block).toContain(`number("${param.key}", "${param.label}", ${param.default.toFixed(Math.max(1, String(param.default).split(".")[1]?.length ?? 0))}, `);
			} else {
				expect(param.default).toBe("#00b140");
			}
		}
	});
});

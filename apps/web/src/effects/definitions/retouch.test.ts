import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { retouchEffectDefinition } from "./library";

const passFor = (params: Record<string, number>, height = 1080) =>
	retouchEffectDefinition.renderer.buildPasses?.({ effectParams: params, width: (height * 16) / 9, height, timeSeconds: 0 })[0];

describe("retouch", () => {
	it("fills the shared shader's uniforms, the ring step in whole pixels", () => {
		const pass = passFor({ smooth: 0.8, brighten: 0.2, even: 0.5, radius: 12 });
		expect(pass?.shader).toBe("retouch");
		expect(pass?.uniforms).toMatchObject({ u_smooth: 0.8, u_brighten: 0.2, u_even: 0.5, u_step: 6, u_face0_rx: 0 });
		expect(Object.keys(pass?.uniforms ?? {})).toHaveLength(16);
		expect(pass?.faces).toBeUndefined();
		expect(passFor({ area: "faces" as unknown as number })?.faces).toBe(true);
		// The radius is given at 1080 high and scales with the frame, as on the desktop.
		expect(passFor({ radius: 12 }, 720)?.uniforms.u_step).toBe(4);
		expect(passFor({ radius: 2 }, 360)?.uniforms.u_step).toBe(1);
		expect(passFor({})?.uniforms).toMatchObject({ u_smooth: 0.6, u_brighten: 0.3, u_even: 0.4, u_step: 6 });
	});

	it("matches the desktop catalogue's keys, labels and defaults", () => {
		const rust = readFileSync(resolve(__dirname, "../../../../../crates/editor/src/effect_catalogue.rs"), "utf8");
		const block = rust.slice(rust.indexOf('kind: "retouch"'), rust.indexOf('kind: "chroma-key"'));
		for (const param of retouchEffectDefinition.params) {
			if (param.type === "number") {
				expect(block).toContain(`number("${param.key}", "${param.label}", ${param.default.toFixed(Math.max(1, String(param.default).split(".")[1]?.length ?? 0))}, `);
			} else if (param.type === "select") {
				expect(block).toContain(`key: "${param.key}"`);
				for (const option of param.options) expect(block).toContain(`("${option.value}", "${option.label}")`);
			}
		}
	});
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { VOICE_EFFECTS, isVoiceEffect, normalizePeak, voiceChannels } from "../apply";

const RATE = 48_000;
const sine = (frequency: number, seconds: number) =>
	Float32Array.from({ length: RATE * seconds }, (_, i) => Math.sin((2 * Math.PI * frequency * i) / RATE) * 0.5);

/** Power at one frequency over the middle half. */
function power(samples: Float32Array, frequency: number): number {
	const from = samples.length / 4;
	const to = (samples.length * 3) / 4;
	let re = 0;
	let im = 0;
	for (let i = from; i < to; i++) {
		const angle = (2 * Math.PI * frequency * (i - from)) / RATE;
		re += samples[i] * Math.cos(angle);
		im += samples[i] * Math.sin(angle);
	}
	return Math.hypot(re, im) / (to - from);
}

describe("voice effects", () => {
	it("lists the same effects as the Rust crate", () => {
		const rust = readFileSync(resolve(__dirname, "../../../../../crates/voice/src/lib.rs"), "utf8");
		for (const effect of VOICE_EFFECTS) expect(rust).toContain(`("${effect.id}", "${effect.label}")`);
		expect(rust.match(/^\s+\("[a-z]+", "[A-Za-z]+"\),$/gm)).toHaveLength(VOICE_EFFECTS.length);
		expect(isVoiceEffect("robot")).toBe(true);
		expect(isVoiceEffect("nope")).toBe(false);
	});

	it("runs the shared code through WebAssembly", () => {
		const input = sine(440, 1);
		const [deeper] = voiceChannels({ channels: [input], sampleRate: RATE, effect: "deeper" });
		expect(deeper.length).toBe(input.length);
		const target = 440 * 2 ** (-4 / 12);
		expect(power(deeper, target)).toBeGreaterThan(5 * power(deeper, 440));
		for (const effect of VOICE_EFFECTS) {
			const [out] = voiceChannels({ channels: [input.slice(0, 4800)], sampleRate: RATE, effect: effect.id });
			expect(out.length).toBe(4800);
			expect(Math.max(...out.map(Math.abs))).toBeLessThanOrEqual(0.98 + 1e-6);
		}
	});

	it("keeps channels together under full scale", () => {
		const channels = [Float32Array.from([2, -1]), Float32Array.from([0.5, 4])];
		expect(normalizePeak({ channels })).toBeCloseTo(0.245, 6);
		expect(channels[1][1]).toBeCloseTo(0.98, 6);
	});
});

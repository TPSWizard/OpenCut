import { describe, expect, it } from "vitest";
import { applyFxInterleaved, audioFxOf, audioFxPresetList, fxKey, fxParams, isIdentityFx, matchingPreset } from "../fx";

const RATE = 48_000;
const sine = (frequency: number, seconds: number, amplitude: number) =>
	Float32Array.from({ length: RATE * seconds }, (_, i) => amplitude * Math.sin((2 * Math.PI * frequency * i) / RATE));
const levelDb = (samples: Float32Array) => {
	const tail = samples.subarray(samples.length / 2);
	let sum = 0;
	for (const value of tail) sum += value * value;
	return 20 * Math.log10(Math.sqrt(sum / tail.length));
};

describe("audio effects through WASM", () => {
	it("reads params like the desktop and leaves audio alone by default", () => {
		expect(audioFxOf({})).toEqual({ low: 0, mid: 0, high: 0, compressor: null, limiter: null });
		expect(isIdentityFx(audioFxOf({}))).toBe(true);
		const fx = audioFxOf({ "audio.eq.low": 30, "audio.comp.enabled": true, "audio.comp.threshold": -18, "audio.limit.enabled": true });
		expect(fx).toEqual({ low: 12, mid: 0, high: 0, compressor: [-18, 4], limiter: -1 });
		expect(audioFxOf(fxParams(fx))).toEqual(fx);
		expect(fxKey(audioFxOf({}))).toBe("");
		const dry = sine(440, 0.1, 0.5);
		expect(applyFxInterleaved({ samples: dry, channels: 1, sampleRate: RATE, fx: audioFxOf({}) })).toBe(dry);
	});

	it("boosts the bass shelf and holds the limiter ceiling", () => {
		const dry = sine(40, 0.5, 0.05);
		const boosted = applyFxInterleaved({ samples: dry, channels: 1, sampleRate: RATE, fx: { ...audioFxOf({}), low: 9 } });
		expect(levelDb(boosted) - levelDb(dry)).toBeGreaterThan(8);
		expect(levelDb(boosted) - levelDb(dry)).toBeLessThan(10);
		// Interleaved stereo, loud: every sample under -6 dB after the limiter.
		const loud = new Float32Array(RATE);
		for (let i = 0; i < RATE / 2; i++) {
			loud[i * 2] = 1.4 * Math.sin((2 * Math.PI * 200 * i) / RATE);
			loud[i * 2 + 1] = 0.3 * Math.sin((2 * Math.PI * 300 * i) / RATE);
		}
		const limited = applyFxInterleaved({ samples: loud, channels: 2, sampleRate: RATE, fx: { ...audioFxOf({}), limiter: -6 } });
		const ceiling = 10 ** (-6 / 20) + 1e-6;
		expect(Math.max(...Array.from(limited, Math.abs))).toBeLessThanOrEqual(ceiling);
	});

	it("lists the shared presets", () => {
		const presets = audioFxPresetList();
		expect(presets.map((preset) => preset.id)).toEqual(["none", "voice-clarity", "podcast", "bass-boost", "bright", "warm", "loud"]);
		expect(matchingPreset(presets[1].fx)).toBe("voice-clarity");
		expect(matchingPreset(audioFxOf({}))).toBe("none");
	});
});

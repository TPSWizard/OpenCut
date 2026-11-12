import { describe, expect, it } from "vitest";
import { FRAME, denoiseChannel, encodeWav, fft, ifft } from "../spectral";

const RATE = 48_000;

/** A deterministic pseudo-random generator for repeatable noise. */
function noise({ seed }: { seed: number }): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state * 1664525 + 1013904223) >>> 0;
		return state / 2 ** 32 - 0.5;
	};
}

/** Energy of the signal in the band `[low, high]` Hz, in dB (Hann-windowed frames, so a tone's leakage stays out of the band). */
function bandDb({ samples, low, high }: { samples: Float32Array; low: number; high: number }): number {
	let energy = 0;
	let frames = 0;
	for (let offset = 0; offset + FRAME <= samples.length; offset += FRAME) {
		const real = new Float64Array(FRAME);
		const imag = new Float64Array(FRAME);
		for (let i = 0; i < FRAME; i++) real[i] = samples[offset + i] * (0.5 - 0.5 * Math.cos((2 * Math.PI * i) / FRAME));
		fft({ real, imag });
		for (let bin = 0; bin <= FRAME / 2; bin++) {
			const hz = (bin * RATE) / FRAME;
			if (hz >= low && hz <= high) energy += real[bin] ** 2 + imag[bin] ** 2;
		}
		frames += 1;
	}
	return 10 * Math.log10(energy / Math.max(1, frames) + 1e-20);
}

describe("fft", () => {
	it("round-trips through the inverse", () => {
		const real = new Float64Array(FRAME);
		const imag = new Float64Array(FRAME);
		const random = noise({ seed: 7 });
		const original = real.map(() => random());
		real.set(original);
		fft({ real, imag });
		ifft({ real, imag });
		for (let i = 0; i < FRAME; i++) expect(real[i]).toBeCloseTo(original[i], 9);
	});
});

describe("denoiseChannel", () => {
	it("takes steady hiss down while keeping the tone", () => {
		const random = noise({ seed: 1 });
		const seconds = 4;
		const clean = new Float32Array(RATE * seconds);
		const noisy = new Float32Array(RATE * seconds);
		// A tone that plays half a second in every second, like speech with pauses, over hiss.
		for (let i = 0; i < clean.length; i++) {
			const on = Math.floor((i / RATE) * 2) % 2 === 0;
			clean[i] = on ? 0.3 * Math.sin((2 * Math.PI * 440 * i) / RATE) : 0;
			noisy[i] = clean[i] + 0.08 * random();
		}
		const result = denoiseChannel({ samples: noisy, options: { reductionDb: 20 } });
		expect(result.length).toBe(noisy.length);
		// Hiss between 3 and 12 kHz drops by well over 10 dB.
		const hissBefore = bandDb({ samples: noisy, low: 3000, high: 12_000 });
		const hissAfter = bandDb({ samples: result, low: 3000, high: 12_000 });
		expect(hissBefore - hissAfter).toBeGreaterThan(10);
		// The tone around 440 Hz keeps its level within 1.5 dB.
		const toneBefore = bandDb({ samples: clean, low: 400, high: 480 });
		const toneAfter = bandDb({ samples: result, low: 400, high: 480 });
		expect(Math.abs(toneAfter - toneBefore)).toBeLessThan(1.5);
	});

	it("leaves silence silent and keeps length", () => {
		const result = denoiseChannel({ samples: new Float32Array(1000) });
		expect(result.length).toBe(1000);
		expect(Math.max(...Array.from(result).map(Math.abs))).toBe(0);
	});
});

describe("encodeWav", () => {
	it("writes a 16-bit PCM header and data", async () => {
		const left = new Float32Array([0, 0.5, -0.5, 1]);
		const right = new Float32Array([0, -0.5, 0.5, -1]);
		const blob = encodeWav({ channels: [left, right], sampleRate: 8000 });
		expect(blob.size).toBe(44 + 4 * 2 * 2);
		const view = new DataView(await blob.arrayBuffer());
		expect(String.fromCharCode(view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3))).toBe("RIFF");
		expect(view.getUint16(22, true)).toBe(2);
		expect(view.getUint32(24, true)).toBe(8000);
		expect(view.getInt16(44 + 2, true)).toBe(0);
		expect(view.getInt16(44 + 4, true)).toBe(16384);
		expect(view.getInt16(44 + 14, true)).toBe(-32767);
	});
});

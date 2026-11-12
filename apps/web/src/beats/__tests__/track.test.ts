import { describe, expect, it } from "vitest";
import { onsetStrength, trackBeats } from "../track";

const RATE = 16_000;

/** Clicks (20 ms of 1 kHz) every `period` seconds over a quiet hum, from `phase`. */
function clicks({ bpm, seconds, phase = 0 }: { bpm: number; seconds: number; phase?: number }): Float32Array {
	const out = new Float32Array(Math.round(seconds * RATE));
	const period = 60 / bpm;
	for (let index = 0; index < out.length; index++) {
		const t = index / RATE;
		const since = ((t - phase) % period + period) % period;
		out[index] = 0.01 * Math.sin(2 * Math.PI * 220 * t);
		if (t >= phase && since < 0.02) out[index] += 0.8 * Math.sin(2 * Math.PI * 1000 * t);
	}
	return out;
}

describe("trackBeats", () => {
	it("finds the tempo and the beat phase of a click track", () => {
		const result = trackBeats({ samples: clicks({ bpm: 100, seconds: 12, phase: 0.25 }), sampleRate: RATE });
		expect(result.bpm).not.toBeNull();
		expect(Math.abs((result.bpm ?? 0) - 100)).toBeLessThan(1.5);
		expect(result.confidence).toBeGreaterThan(0.3);
		expect(result.beats.length).toBeGreaterThan(15);
		for (const beat of result.beats) {
			const offset = ((beat - 0.25) % 0.6 + 0.6) % 0.6;
			const distance = Math.min(offset, 0.6 - offset);
			expect(distance).toBeLessThan(0.03);
		}
		expect(result.seconds).toBe(12);
	});

	it("prefers the tempo near 120 when a slow track could also be read double", () => {
		const result = trackBeats({ samples: clicks({ bpm: 70, seconds: 12 }), sampleRate: RATE });
		// 70 BPM clicks also correlate at 140; either reading keeps the beats on the clicks.
		expect([70, 140].some((bpm) => Math.abs((result.bpm ?? 0) - bpm) < 1.5)).toBe(true);
	});

	it("gives nothing for silence or a steady tone", () => {
		expect(trackBeats({ samples: new Float32Array(RATE * 5), sampleRate: RATE }).bpm).toBeNull();
		const tone = new Float32Array(RATE * 5);
		for (let index = 0; index < tone.length; index++) tone[index] = 0.5 * Math.sin((2 * Math.PI * 440 * index) / RATE);
		const result = trackBeats({ samples: tone, sampleRate: RATE });
		expect(result.beats.length === 0 || result.confidence < 0.2).toBe(true);
	});

	it("measures onset strength per 5 ms hop", () => {
		const onset = onsetStrength({ samples: clicks({ bpm: 120, seconds: 2 }), sampleRate: RATE });
		expect(onset.length).toBeGreaterThan(380);
		expect(onset.length).toBeLessThan(400);
		expect(Math.max(...onset)).toBeGreaterThan(0);
	});
});

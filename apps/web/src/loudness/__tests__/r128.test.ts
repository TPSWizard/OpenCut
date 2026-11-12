import { describe, expect, it } from "vitest";
import {
	kWeighting,
	loudnessRange,
	measureLoudness,
	normalizationGain,
	oversamplingPhases,
	truePeak,
} from "../r128";

const RATE = 48_000;

function sine({
	seconds,
	amplitude,
	frequency = 1000,
	sampleRate = RATE,
}: {
	seconds: number;
	amplitude: number;
	frequency?: number;
	sampleRate?: number;
}): Float32Array {
	const out = new Float32Array(Math.round(seconds * sampleRate));
	for (let index = 0; index < out.length; index++) {
		out[index] = amplitude * Math.sin((2 * Math.PI * frequency * index) / sampleRate);
	}
	return out;
}

function db(amplitude: number): number {
	return 20 * Math.log10(amplitude);
}

describe("kWeighting", () => {
	it("reproduces the standard's 48 kHz coefficients", () => {
		const [shelf, highPass] = kWeighting({ sampleRate: 48_000 });
		expect(shelf.b0).toBeCloseTo(1.53512485958697, 6);
		expect(shelf.b1).toBeCloseTo(-2.69169618940638, 6);
		expect(shelf.b2).toBeCloseTo(1.19839281085285, 6);
		expect(shelf.a1).toBeCloseTo(-1.69065929318241, 6);
		expect(shelf.a2).toBeCloseTo(0.73248077421585, 6);
		expect(highPass.a1).toBeCloseTo(-1.99004745483398, 6);
		expect(highPass.a2).toBeCloseTo(0.99007225036621, 6);
	});
});

describe("measureLoudness", () => {
	it("reads a 1 kHz sine 3 dB below its RMS, in stereo and at 44.1 kHz", () => {
		// BS.1770: a full-scale 1 kHz sine is −3.01 LUFS, so amplitude a gives 20·log10(a) − 3.01.
		const amplitude = 0.0125;
		const expected = db(amplitude) - 3.01;
		const stereo = measureLoudness({ channels: [sine({ seconds: 5, amplitude }), sine({ seconds: 5, amplitude })], sampleRate: RATE });
		expect(stereo.integratedLufs).toBeCloseTo(expected + 3.01, 0.5);
		expect(stereo.integratedLufs).toBeGreaterThan(expected + 2.9);
		expect(stereo.integratedLufs).toBeLessThan(expected + 3.1);
		expect(stereo.loudnessRangeLu).toBeLessThan(0.2);
		expect(stereo.truePeakDbtp).toBeGreaterThan(db(amplitude) - 0.1);
		expect(stereo.truePeakDbtp).toBeLessThan(db(amplitude) + 0.2);
		expect(stereo.maxMomentaryLufs).toBeCloseTo(stereo.integratedLufs, 1);
		expect(stereo.seconds).toBe(5);

		const mono = measureLoudness({ channels: [sine({ seconds: 5, amplitude })], sampleRate: RATE });
		expect(mono.integratedLufs).toBeGreaterThan(expected - 0.1);
		expect(mono.integratedLufs).toBeLessThan(expected + 0.1);

		const at441 = measureLoudness({ channels: [sine({ seconds: 5, amplitude, sampleRate: 44_100 })], sampleRate: 44_100 });
		expect(at441.integratedLufs).toBeGreaterThan(expected - 0.15);
		expect(at441.integratedLufs).toBeLessThan(expected + 0.15);
	});

	it("gates out quiet passages and silence", () => {
		const loud = sine({ seconds: 2, amplitude: 0.1 });
		const quiet = sine({ seconds: 2, amplitude: 0.001 });
		const signal = new Float32Array(loud.length * 2 + quiet.length);
		signal.set(loud, 0);
		signal.set(quiet, loud.length);
		signal.set(loud, loud.length + quiet.length);
		const report = measureLoudness({ channels: [signal], sampleRate: RATE });
		const loudLufs = db(0.1) - 3.01;
		// The quiet middle (40 dB down) is below the relative gate and does not drag the figure down.
		expect(report.integratedLufs).toBeGreaterThan(loudLufs - 0.6);
		expect(report.integratedLufs).toBeLessThan(loudLufs + 0.1);
		expect(report.loudnessRangeLu).toBeLessThan(4);

		const silence = measureLoudness({ channels: [new Float32Array(RATE)], sampleRate: RATE });
		expect(silence.integratedLufs).toBe(-Infinity);
		expect(silence.truePeakDbtp).toBe(-Infinity);
		expect(silence.loudnessRangeLu).toBe(0);
		expect(normalizationGain({ report: silence, targetLufs: -14, truePeakDbtp: -1 })).toBeNull();
	});

	it("reports a range when the level steps", () => {
		const parts = [0.2, 0.02, 0.2, 0.02].map((amplitude) => sine({ seconds: 4, amplitude }));
		const signal = new Float32Array(parts.reduce((sum, part) => sum + part.length, 0));
		let offset = 0;
		for (const part of parts) {
			signal.set(part, offset);
			offset += part.length;
		}
		const report = measureLoudness({ channels: [signal], sampleRate: RATE });
		expect(report.loudnessRangeLu).toBeGreaterThan(10);
		expect(report.loudnessRangeLu).toBeLessThan(21);
	});
});

describe("truePeak", () => {
	it("finds a peak between samples", () => {
		// A sine at a quarter of the sample rate sampled at its zero crossings and ±1/√2 points
		// never hits its true peak of 0.5; oversampling should find it.
		const signal = new Float32Array(4800);
		for (let index = 0; index < signal.length; index++) {
			signal[index] = 0.5 * Math.sin((2 * Math.PI * (RATE / 4) * index) / RATE + Math.PI / 4);
		}
		const sampled = Math.max(...Array.from(signal).map(Math.abs));
		expect(sampled).toBeLessThan(0.36);
		const peak = truePeak({ channels: [signal], sampleRate: RATE });
		expect(peak).toBeGreaterThan(db(0.5) - 0.3);
		expect(peak).toBeLessThan(db(0.5) + 0.3);
		const phases = oversamplingPhases({ factor: 4, taps: 12 });
		expect(phases).toHaveLength(4);
		for (const phase of phases) {
			expect(Array.from(phase).reduce((sum, value) => sum + value, 0)).toBeCloseTo(1, 9);
		}
	});
});

describe("normalizationGain", () => {
	it("aims at the target and backs off for the true-peak ceiling", () => {
		const report = measureLoudness({ channels: [sine({ seconds: 3, amplitude: 0.05 })], sampleRate: RATE });
		const free = normalizationGain({ report, targetLufs: -23, truePeakDbtp: -1 });
		expect(free?.peakLimited).toBe(false);
		expect(free?.gainDb).toBeCloseTo(-23 - report.integratedLufs, 5);
		const limited = normalizationGain({ report, targetLufs: -2, truePeakDbtp: -1 });
		expect(limited?.peakLimited).toBe(true);
		expect(limited?.gainDb).toBeCloseTo(-1 - report.truePeakDbtp, 5);
	});

	it("keeps a constant's range at zero", () => {
		expect(loudnessRange({ shortTerm: [-20, -20, -20] })).toBe(0);
		expect(loudnessRange({ shortTerm: [] })).toBe(0);
	});
});

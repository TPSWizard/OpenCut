import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { masterMix } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { MixMeter, formatMeterLufs, meterFraction, mixLoudnessTarget } from "../mix";

const root = resolve(__dirname, "../../../../..");
type Recipe = { rate: number; frames: number; channels: Record<string, any>[][] };
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/loudness/cases.json"), "utf8")) as { cases: any[] };

/** The recipe's channels, as the reference and the Rust test build them. */
function signal(recipe: Recipe): Float32Array[] {
	return recipe.channels.map((components) => {
		const channel = new Float64Array(recipe.frames);
		for (const component of components) {
			if (component.sine) {
				const { freq, amp, phase = 0 } = component.sine;
				const from = component.sine.from ?? 0;
				const to = Math.min(component.sine.to ?? recipe.frames, recipe.frames);
				for (let i = from; i < to; i++) channel[i] += amp * Math.sin((2 * Math.PI * freq * i) / recipe.rate + phase);
			}
			if (component.clicks) for (const [index, value] of component.clicks) channel[index] += value;
			if (component.noise) {
				let state = BigInt(component.noise.seed);
				for (let i = 0; i < recipe.frames; i++) {
					state = (state * 1103515245n + 12345n) % 2147483648n;
					channel[i] += component.noise.amp * ((Number(state) / 2147483648) * 2 - 1);
				}
			}
		}
		return Float32Array.from(channel, (value) => Math.min(1, Math.max(-1, Math.round(value * 32767) / 32767)));
	});
}

const level = (value: number | null) => (value === null ? -Infinity : value);

describe("whole-mix loudness through WASM", () => {
	it("masters like the reference", () => {
		const cases = fixture.cases.filter((c) => c.kind === "master");
		expect(cases.length).toBeGreaterThanOrEqual(6);
		for (const c of cases) {
			const channels = signal(c.signal);
			const right = channels[1] ?? new Float32Array(0);
			const [inputLufs, inputTp, gainDb, outputLufs, outputTp, limited, passes] = masterMix(
				channels[0],
				right,
				c.signal.rate,
				c.targetLufs,
				c.ceilingDbtp,
			);
			const near = (got: number, want: number | null, digits: number) =>
				want === null ? expect(got, c.name).toBe(-Infinity) : expect(got, c.name).toBeCloseTo(want, digits);
			near(inputLufs, c.report.inputLufs, 6);
			near(inputTp, c.report.inputTruePeakDbtp, 6);
			near(gainDb, c.report.gainDb, 6);
			near(outputLufs, c.report.outputLufs, 4);
			near(outputTp, c.report.outputTruePeakDbtp, 4);
			expect(limited === 1, c.name).toBe(c.report.limited);
			expect(passes, c.name).toBe(c.report.passes);
			const mastered = right.length > 0 ? [channels[0], right] : [channels[0]];
			mastered.forEach((channel, index) => {
				const sum = channel.reduce((total, sample) => total + Math.abs(sample), 0);
				expect(Math.abs(sum - c.sumAbs[index]), c.name).toBeLessThan(1e-3);
				c.picks.forEach((pick: number, at: number) => expect(channel[pick], c.name).toBeCloseTo(c.samples[index][at], 6));
			});
		}
	});

	it("meters like the reference, chunked as played", () => {
		for (const c of fixture.cases.filter((c) => c.kind === "meter")) {
			const channels = signal(c.signal);
			const meter = new MixMeter({ sampleRate: c.signal.rate, channels: channels.length });
			let at = 0;
			c.readings.forEach((want: any, index: number) => {
				const end = Math.min(channels[0].length, at + c.chunks[index % c.chunks.length]);
				meter.push(channels[0].subarray(at, end), channels[1]?.subarray(at, end));
				at = end;
				const got = meter.readings();
				expect(got.momentaryLufs, c.name).toBeCloseTo(level(want.momentaryLufs), 6);
				expect(got.shortTermLufs, c.name).toBeCloseTo(level(want.shortTermLufs), 6);
				if (want.peakDbfs === null) expect(got.peakDbfs).toBe(-Infinity);
				else expect(got.peakDbfs, c.name).toBeCloseTo(want.peakDbfs, 6);
			});
			meter.reset();
			expect(meter.readings().momentaryLufs).toBe(-Infinity);
			meter.dispose();
		}
	});

	it("names the targets and formats the meter", () => {
		expect(mixLoudnessTarget("streaming")).toEqual({ targetLufs: -14, ceilingDbtp: -1 });
		expect(mixLoudnessTarget("broadcast")).toEqual({ targetLufs: -23, ceilingDbtp: -1 });
		expect(mixLoudnessTarget("off")).toBeUndefined();
		expect(formatMeterLufs(-14.04)).toBe("-14.0");
		expect(formatMeterLufs(-1280)).toBe("–");
		expect(formatMeterLufs(-Infinity)).toBe("–");
		expect(meterFraction(-30)).toBeCloseTo(0.5, 12);
		expect(meterFraction(3)).toBe(1);
	});
});

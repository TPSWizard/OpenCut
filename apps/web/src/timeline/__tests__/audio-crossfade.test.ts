import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { audioCrossfades, planCrossfade } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { crossfadedElements, curveGain, getClipCrossfade, planElementCrossfade } from "../audio-crossfade";
import { fadeGainAt, resolveEffectiveAudioGain, type AudioCapableElement } from "../audio-state";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/crossfade/cases.json"), "utf8")) as {
	cases: {
		name: string;
		clips: unknown[];
		joins: unknown[];
		played: (Record<string, unknown> | null)[];
		plan?: { index: number; ticks: number; result: Record<string, unknown> };
	}[];
};

const S = 120_000;

function audio(id: string, start: number, length: number, params: Record<string, unknown> = {}): AudioCapableElement {
	return {
		id,
		name: id,
		type: "audio",
		sourceType: "upload",
		mediaId: `m-${id}`,
		startTime: Math.round(start * S),
		duration: Math.round(length * S),
		trimStart: 0,
		trimEnd: 0,
		params,
	} as unknown as AudioCapableElement;
}

describe("audio crossfades through WASM", () => {
	it("match the reference", () => {
		for (const c of fixture.cases) {
			const played = JSON.parse(audioCrossfades(JSON.stringify(c.clips))) as (Record<string, unknown> | null)[];
			expect(played.length, c.name).toBe(c.played.length);
			played.forEach((got, index) => {
				const want = c.played[index];
				if (!want) return expect(got, c.name).toBeNull();
				expect(got, c.name).toMatchObject({ ...want, fadeIn: expect.any(Number), fadeOut: expect.any(Number) });
				expect(got?.fadeIn as number, c.name).toBeCloseTo(want.fadeIn as number, 12);
				expect(got?.fadeOut as number, c.name).toBeCloseTo(want.fadeOut as number, 12);
			});
			if (!c.plan) continue;
			const result = JSON.parse(planCrossfade(JSON.stringify(c.clips), c.plan.index, c.plan.ticks));
			if ("error" in c.plan.result) expect(result.error, c.name).toBe(c.plan.result.error);
			else expect(result.ok, c.name).toEqual(c.plan.result);
		}
	});

	it("plans, plays and crosses at equal power like the desktop", () => {
		// The editor crate's test: two whole 4 s files, then a 2 s one.
		const elements = [audio("a0", 0, 4), audio("a1", 4, 4), audio("a2", 8, 2)];
		const plan = planElementCrossfade(elements, 0, 1);
		expect([plan.length, plan.shift, plan.moved]).toEqual([S, S, [2]]);
		const edited = [
			{ ...elements[0], duration: plan.fromEdit.duration, trimEnd: plan.fromEdit.trimEnd, params: { crossfade: 1, crossfadeCurve: "equalPower" } },
			{ ...elements[1], startTime: plan.toEdit.start, duration: plan.toEdit.duration, trimStart: plan.toEdit.trimStart },
			{ ...elements[2], startTime: elements[2].startTime - plan.shift },
		] as AudioCapableElement[];
		expect(edited.map((element) => [element.startTime / S, element.duration / S])).toEqual([
			[0, 3.5],
			[3.5, 3.5],
			[7, 2],
		]);
		expect(getClipCrossfade(edited[0])).toEqual({ seconds: 1, curve: "equalPower" });

		const [first, second, third] = crossfadedElements(edited);
		expect([first.startTime / S, (first.startTime + first.duration) / S, first.trimEnd]).toEqual([0, 4, 0]);
		expect([second.startTime / S, second.trimStart]).toEqual([3, 0]);
		expect(third).toBe(edited[2]);
		for (let step = 0; step <= 10; step++) {
			const time = 3 + step / 10;
			const out = resolveEffectiveAudioGain({ element: first, localTime: time - first.startTime / S });
			const into = resolveEffectiveAudioGain({ element: second, localTime: time - second.startTime / S });
			expect(out * out + into * into).toBeCloseTo(1, 6);
		}
		expect(fadeGainAt({ element: first, localTime: 3.5 })).toBeCloseTo(Math.SQRT1_2, 6);
	});

	it("leaves tracks without crossfades alone and keeps plain fades straight", () => {
		const elements = [audio("a", 0, 4, { fadeOut: 2 }), audio("b", 4, 4)];
		expect(crossfadedElements(elements)).toBe(elements);
		expect(fadeGainAt({ element: elements[0], localTime: 3 })).toBeCloseTo(0.5, 12);
		expect(curveGain("equalPower", 0.5)).toBeCloseTo(Math.SQRT1_2, 12);
		expect(curveGain("linear", 0.25)).toBe(0.25);
		expect(() => planElementCrossfade(elements, 1, 1)).toThrow("No audio or video clip");
	});
});

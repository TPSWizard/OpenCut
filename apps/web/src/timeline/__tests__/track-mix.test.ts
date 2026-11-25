import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { panStereo } from "../audio-state";
import { getTrackMixSettings, trackMixes } from "../track-mix";
import type { TimelineTrack } from "../types";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/trackmix/cases.json"), "utf8")) as {
	cases: { name: string; tracks: { muted: boolean; solo: boolean; volume: number; pan: number; sounding: boolean }[]; mix: { audible: boolean; gain: number; pan: number }[] }[];
	pans: { clipPan: number; trackPan: number; in: [number, number]; out: [number, number] }[];
};

function track(index: number, settings: { muted: boolean; solo: boolean; volume: number; pan: number; sounding: boolean }): TimelineTrack {
	return settings.sounding
		? ({ id: `t${index}`, name: `t${index}`, type: "audio", elements: [], muted: settings.muted, solo: settings.solo, volume: settings.volume, pan: settings.pan } as TimelineTrack)
		: ({ id: `t${index}`, name: `t${index}`, type: "text", elements: [], hidden: false, solo: settings.solo } as unknown as TimelineTrack);
}

describe("track mixing through WASM", () => {
	it("matches the reference", () => {
		for (const c of fixture.cases) {
			const tracks = c.tracks.map((settings, index) => track(index, settings));
			const mixes = trackMixes(tracks);
			c.mix.forEach((want, index) => {
				const got = mixes.get(`t${index}`)!;
				expect(got.audible, c.name).toBe(want.audible);
				expect(got.gain, c.name).toBeCloseTo(want.gain, 12);
				expect(got.pan, c.name).toBeCloseTo(want.pan, 12);
			});
		}
	});

	it("stacks the clip's pan and the track's like the reference", () => {
		for (const pan of fixture.pans) {
			const [left, right] = panStereo({ pan: pan.clipPan, left: pan.in[0], right: pan.in[1] });
			const out = panStereo({ pan: pan.trackPan, left, right });
			expect(out[0]).toBeCloseTo(pan.out[0], 12);
			expect(out[1]).toBeCloseTo(pan.out[1], 12);
		}
	});

	it("reads stored settings with their defaults", () => {
		const audio = { id: "a", name: "a", type: "audio", elements: [], muted: false } as TimelineTrack;
		expect(getTrackMixSettings(audio)).toEqual({ muted: false, solo: false, volume: 0, pan: 0 });
		const text = { id: "x", name: "x", type: "text", elements: [], hidden: false } as TimelineTrack;
		expect(trackMixes([audio, text]).get("x")).toEqual({ audible: false, gain: 0, pan: 0 });
	});
});

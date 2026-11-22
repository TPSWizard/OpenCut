import { describe, expect, it } from "vitest";
import { TICKS_PER_SECOND, type MediaTime } from "@/wasm";
import {
	type AudioCapableElement,
	buildAudioGainAutomation,
	fadeGainAt,
	getAudioFades,
	getElementPan,
	hasGainAutomation,
	panStereo,
	resolveEffectiveAudioGain,
} from "../audio-state";

const clip = (params: Record<string, number | boolean>, seconds = 10): AudioCapableElement =>
	({
		id: "a",
		type: "audio",
		sourceType: "upload",
		name: "a",
		mediaId: "m",
		startTime: 0 as MediaTime,
		duration: Math.round(seconds * TICKS_PER_SECOND) as MediaTime,
		trimStart: 0 as MediaTime,
		trimEnd: 0 as MediaTime,
		params: { volume: 0, muted: false, ...params },
	}) as AudioCapableElement;

describe("audio fades", () => {
	it("ramps up over the fade in and down over the fade out", () => {
		const element = clip({ fadeIn: 2, fadeOut: 4 });
		expect(hasGainAutomation({ element })).toBe(true);
		expect(fadeGainAt({ element, localTime: 0 })).toBe(0);
		expect(fadeGainAt({ element, localTime: 1 })).toBeCloseTo(0.5, 9);
		expect(fadeGainAt({ element, localTime: 4 })).toBe(1);
		expect(fadeGainAt({ element, localTime: 8 })).toBeCloseTo(0.5, 9);
		expect(fadeGainAt({ element, localTime: 10 })).toBe(0);
		// The fade multiplies the volume (-6 dB here).
		const quieter = clip({ fadeIn: 2, volume: -6 });
		expect(resolveEffectiveAudioGain({ element: quieter, localTime: 1 })).toBeCloseTo(0.5 * 10 ** (-6 / 20), 9);
		const points = buildAudioGainAutomation({ element, fromLocalTime: 0, toLocalTime: 10, stepSeconds: 0.5 });
		expect(points[0].gain).toBe(0);
		expect(points.at(-1)?.gain).toBe(0);
		expect(Math.max(...points.map((point) => point.gain))).toBe(1);
	});

	it("keeps fades inside the clip and within 10 s", () => {
		expect(getAudioFades({ element: clip({ fadeIn: 3, fadeOut: 3 }, 4) })).toEqual({ fadeIn: 2, fadeOut: 2 });
		expect(getAudioFades({ element: clip({ fadeIn: 60, fadeOut: -2 }, 100) })).toEqual({ fadeIn: 10, fadeOut: 0 });
		expect(hasGainAutomation({ element: clip({}) })).toBe(false);
		expect(fadeGainAt({ element: clip({}), localTime: 0 })).toBe(1);
	});
});

describe("stereo pan", () => {
	it("matches Web Audio's two-channel StereoPannerNode", () => {
		expect(panStereo({ pan: 0, left: 0.3, right: 0.7 })).toEqual([0.3, 0.7]);
		const hardLeft = panStereo({ pan: -1, left: 0.3, right: 0.7 });
		expect(hardLeft[0]).toBeCloseTo(1, 9);
		expect(hardLeft[1]).toBeCloseTo(0, 9);
		const hardRight = panStereo({ pan: 1, left: 0.3, right: 0.7 });
		expect(hardRight[0]).toBeCloseTo(0, 9);
		expect(hardRight[1]).toBeCloseTo(1, 9);
		const half = panStereo({ pan: 0.5, left: 1, right: 1 });
		expect(half[0]).toBeCloseTo(Math.SQRT1_2, 9);
		expect(half[1]).toBeCloseTo(1 + Math.SQRT1_2, 9);
		expect(getElementPan({ element: clip({ pan: 3 }) })).toBe(1);
		expect(getElementPan({ element: clip({}) })).toBe(0);
	});
});

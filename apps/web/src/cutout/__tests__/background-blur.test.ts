import { describe, expect, it } from "vitest";
import { backgroundBlurElement } from "@/services/renderer/scene-builder";
import type { TimelineElement } from "@/timeline/types";

function clip(cutout?: Record<string, unknown>, effects: unknown[] = []): TimelineElement {
	return {
		id: "v",
		name: "v",
		type: "video",
		mediaId: "m",
		startTime: 0,
		duration: 120000,
		trimStart: 0,
		trimEnd: 0,
		params: {},
		effects,
		cutout,
	} as unknown as TimelineElement;
}

describe("background blur (portrait mode)", () => {
	it("draws the whole clip blurred under the cut-out, as the desktop", () => {
		const own = { id: "c", type: "color", params: { brightness: 0.1 }, enabled: true };
		const background = backgroundBlurElement(clip({ matteId: "matte", offset: 0, backgroundBlur: 60 }, [own]));
		expect(background).not.toBeNull();
		expect((background as { cutout?: unknown }).cutout).toBeUndefined();
		expect((background as { effects: unknown[] }).effects).toEqual([
			own,
			{ id: "background-blur", type: "blur", params: { intensity: 60 }, enabled: true },
		]);
		// Capped at the Blur effect's 100.
		const strong = backgroundBlurElement(clip({ matteId: "matte", offset: 0, backgroundBlur: 400 }));
		expect((strong as unknown as { effects: { params: { intensity: number } }[] }).effects[0].params.intensity).toBe(100);
	});

	it("adds nothing without a cut-out or a blur", () => {
		expect(backgroundBlurElement(clip())).toBeNull();
		expect(backgroundBlurElement(clip({ matteId: "matte", offset: 0 }))).toBeNull();
		expect(backgroundBlurElement(clip({ matteId: "matte", offset: 0, backgroundBlur: 0 }))).toBeNull();
	});
});

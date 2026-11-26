import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { trimEditPlan } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import type { TimelineTrack } from "../types";
import { planTrimEdit, rollTarget } from "../trim-tools";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/trimedit/cases.json"), "utf8")) as {
	cases: {
		name: string;
		tool: string;
		target: string;
		delta: number;
		minDuration: number;
		track: { id: string; start: number; duration: number; trimStart: number; trimEnd: number; limited: boolean; retimed: boolean }[];
		plan?: unknown;
		error?: boolean;
	}[];
};

describe("slip, slide and roll through WASM", () => {
	it("plans every case as the reference (and the desktop)", () => {
		for (const item of fixture.cases) {
			const run = () => JSON.parse(trimEditPlan(item.tool, item.target, JSON.stringify(item.track), item.delta, item.minDuration));
			if (item.error) expect(run, item.name).toThrow();
			else expect(run(), item.name).toEqual(item.plan);
		}
	});

	it("reads the web's clips: media clips are limited, speed changes refused", () => {
		const element = (id: string, type: string, start: number, extra: Record<string, unknown> = {}) => ({
			id,
			type,
			startTime: start,
			duration: 120000,
			trimStart: 60000,
			trimEnd: 60000,
			...extra,
		});
		const track = { id: "t", elements: [element("a", "video", 0), element("b", "image", 120000), element("c", "video", 240000, { retime: { rate: 2 } })] } as unknown as TimelineTrack;
		expect(planTrimEdit({ tool: "slip", track, elementId: "a", deltaTicks: 999999, minDuration: 4000 }).delta).toBe(60000);
		expect(() => planTrimEdit({ tool: "slip", track, elementId: "b", deltaTicks: 1000, minDuration: 4000 })).toThrow();
		expect(() => planTrimEdit({ tool: "slip", track, elementId: "c", deltaTicks: 1000, minDuration: 4000 })).toThrow();
		expect(rollTarget({ track, element: track.elements[1] as never, laterHalf: false })).toBe("a");
		expect(rollTarget({ track, element: track.elements[1] as never, laterHalf: true })).toBe("b");
	});
});

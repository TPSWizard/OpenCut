import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { multicamLiveAngle } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import type { MediaAsset } from "@/media/types";
import type { SceneTracks } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { gridCellAt, gridCells, multicamGridTracks } from "../grid";
import { MULTICAM_ANGLE, MULTICAM_CUTS, MULTICAM_GROUP } from "../group";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/multicam/grid.json"), "utf8")) as {
	grids: Array<{
		name: string;
		canvas: [number, number];
		sources: [number, number][];
		placements: Array<{ cell: [number, number, number, number]; positionX: number; positionY: number; scale: number }>;
		hits: [number, number, number][];
	}>;
	live: Array<{ cuts: string; probes: [number, number][] }>;
};

describe("multicam grid", () => {
	it("lays the grid out as the reference does, through WASM", () => {
		expect(fixture.grids.length).toBeGreaterThanOrEqual(8);
		for (const grid of fixture.grids) {
			const canvas = { width: grid.canvas[0], height: grid.canvas[1] };
			const cells = gridCells(canvas, grid.sources.map(([width, height]) => ({ width, height })));
			expect(cells.length, grid.name).toBe(grid.placements.length);
			cells.forEach((cell, index) => {
				const want = grid.placements[index];
				[cell.x, cell.y, cell.width, cell.height].forEach((value, part) => expect(value).toBeCloseTo(want.cell[part], 9));
				expect(cell.positionX).toBeCloseTo(want.positionX, 9);
				expect(cell.positionY).toBeCloseTo(want.positionY, 9);
				expect(cell.scale).toBeCloseTo(want.scale, 9);
			});
			for (const [x, y, want] of grid.hits) {
				expect(gridCellAt(grid.sources.length, canvas, x, y) ?? -1, `${grid.name} at ${x},${y}`).toBe(want);
			}
		}
		for (const live of fixture.live) {
			for (const [time, angle] of live.probes) expect(multicamLiveAngle(live.cuts, time), `${live.cuts} at ${time}`).toBe(angle);
		}
	});

	it("draws only the angles, each fitted into its cell and fully opaque", () => {
		const video = (id: string, angle: number) => ({
			id,
			type: "video",
			name: `${id}.mp4`,
			mediaId: `m-${id}`,
			startTime: 0,
			duration: 10 * TICKS_PER_SECOND,
			trimStart: 0,
			trimEnd: 0,
			params: { [MULTICAM_GROUP]: "g", [MULTICAM_ANGLE]: angle, [MULTICAM_CUTS]: "2.000:1", opacity: 0, blendMode: "screen", "transform.positionX": 50 },
			animations: { opacity: { keys: [] }, volume: { keys: [] } },
		});
		const text = { id: "title", type: "text", name: "Title", startTime: 0, duration: TICKS_PER_SECOND, params: {} };
		const tracks = {
			overlay: [{ id: "t2", elements: [video("b", 1)] }, { id: "t3", elements: [text] }],
			main: { id: "t1", elements: [video("a", 0)] },
			audio: [],
		} as unknown as SceneTracks;
		const assets = [
			{ id: "m-a", width: 1920, height: 1080 },
			{ id: "m-b", width: 1080, height: 1920 },
		] as unknown as MediaAsset[];
		const canvas = { width: 1920, height: 1080 };
		const grid = multicamGridTracks({ tracks, elementId: "b", assets, canvas })!;
		const cells = gridCells(canvas, [
			{ width: 1920, height: 1080 },
			{ width: 1080, height: 1920 },
		]);
		const a = grid.main.elements[0] as unknown as { params: Record<string, unknown>; animations: Record<string, unknown> };
		const b = grid.overlay[0].elements[0] as unknown as { params: Record<string, unknown> };
		expect(a.params).toMatchObject({ opacity: 1, blendMode: "normal", "transform.positionX": cells[0].positionX, "transform.scaleX": cells[0].scale });
		expect(b.params).toMatchObject({ "transform.positionX": cells[1].positionX, "transform.scaleY": cells[1].scale });
		expect(a.animations.opacity).toBeUndefined();
		expect(a.animations.volume).toBeDefined();
		expect(grid.overlay[1].elements).toHaveLength(0);
		// The timeline itself is untouched.
		expect((tracks.main.elements[0] as unknown as { params: Record<string, unknown> }).params.opacity).toBe(0);
		expect(multicamGridTracks({ tracks, elementId: "title", assets, canvas })).toBeNull();
	});
});

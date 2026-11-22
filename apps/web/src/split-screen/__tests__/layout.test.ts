import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { SPLIT_LAYOUTS, layoutsFor, planSplitScreen, splitLayout, type SplitPlacement } from "../layout";

const root = resolve(__dirname, "../../../../..");

describe("split-screen layouts", () => {
	it("places the shared cases like the desktop", () => {
		const { cases } = JSON.parse(readFileSync(resolve(root, "fixtures/split-screen/cases.json"), "utf8")) as {
			cases: { name: string; layout: string; canvas: [number, number]; gap: number; sources: [number, number][]; placements: SplitPlacement[] }[];
		};
		expect(cases.length).toBeGreaterThan(3);
		for (const { name, layout, canvas, gap, sources, placements } of cases) {
			const planned = planSplitScreen({
				layout: splitLayout(layout)!,
				canvasWidth: canvas[0],
				canvasHeight: canvas[1],
				sources: sources.map(([width, height]) => ({ width, height })),
				gap,
			});
			planned.forEach((placement, index) => {
				for (const key of Object.keys(placements[index]) as (keyof SplitPlacement)[]) {
					expect(placement[key], `${name} #${index} ${key}`).toBeCloseTo(placements[index][key], 9);
				}
			});
		}
	});

	it("covers each cell exactly", () => {
		const [left] = planSplitScreen({ layout: splitLayout("side-by-side")!, canvasWidth: 1920, canvasHeight: 1080, sources: [{ width: 1920, height: 1080 }, { width: 1920, height: 1080 }] });
		expect(left).toEqual({ positionX: -480, positionY: 0, scale: 1, maskWidth: 0.5, maskHeight: 1 });
	});

	it("offers layouts by clip count and checks the count", () => {
		expect(layoutsFor(2).map((layout) => layout.id)).toEqual(["side-by-side", "top-bottom"]);
		expect(layoutsFor(3)).toHaveLength(3);
		expect(layoutsFor(4).map((layout) => layout.id)).toEqual(["grid"]);
		expect(layoutsFor(5)).toEqual([]);
		expect(() => planSplitScreen({ layout: SPLIT_LAYOUTS[0], canvasWidth: 10, canvasHeight: 10, sources: [{ width: 1, height: 1 }] })).toThrow(
			"needs 2 clips",
		);
	});
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { drawingAddStroke, drawingRemoveLastStroke, drawingRevealProgress, drawingStrokeCount, rasteriseDrawing } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { DRAWING_DEFINITION, drawingGraphicDefinition } from "@/graphics/definitions/drawing";
import { DRAW_TOOL_WIDTHS, brushFor, isDrawing } from "../strokes";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/strokes/cases.json"), "utf8")) as {
	cases: { name: string; drawing: string; width: number; height: number; progress: number; rgba: number[] }[];
};

describe("freehand drawing through WASM", () => {
	it("rasterises as the reference (and the desktop)", () => {
		for (const item of fixture.cases) {
			const got = rasteriseDrawing(item.drawing, item.width, item.height, item.progress);
			expect(got.length, item.name).toBe(item.rgba.length);
			let worst = 0;
			got.forEach((value, index) => {
				worst = Math.max(worst, Math.abs(value - item.rgba[index]));
			});
			expect(worst, item.name).toBeLessThanOrEqual(1);
		}
	});

	it("times each stroke after the last and takes the last away", () => {
		const stroke = (start: number) =>
			JSON.stringify({ tool: "pen", color: "#ff3b30", width: 0.01, points: [[0.1, 0.2, 0.5, start], [0.3, 0.25, 0.8, start + 0.4]] });
		const once = drawingAddStroke("", stroke(12));
		const twice = drawingAddStroke(once, stroke(30));
		const parsed = JSON.parse(twice) as { strokes: { points: number[][] }[] };
		expect(parsed.strokes[0].points[0][3]).toBe(0);
		expect(parsed.strokes[1].points[0][3]).toBe(0.55);
		expect(drawingStrokeCount(twice)).toBe(2);
		expect(drawingStrokeCount(drawingRemoveLastStroke(twice))).toBe(1);
		expect(() => drawingAddStroke("", JSON.stringify({ tool: "crayon", color: "#fff", width: 0.1, points: [[0, 0]] }))).toThrow();
		expect(drawingRevealProgress(2, 1)).toBe(0.5);
		expect(drawingRevealProgress(0, 1)).toBe(1);
	});

	it("is a hidden graphic that covers the canvas and is drawn on", () => {
		expect(drawingGraphicDefinition.id).toBe(DRAWING_DEFINITION);
		expect(drawingGraphicDefinition.hidden).toBe(true);
		expect(drawingGraphicDefinition.sourceSize?.({ width: 3840, height: 2160 })).toEqual({ width: 1920, height: 1080 });
		expect(drawingGraphicDefinition.sourceSize?.({ width: 1280, height: 720 })).toEqual({ width: 1280, height: 720 });
		expect(drawingGraphicDefinition.timedParams?.({ drawOn: 4 }, 1)).toEqual({ __reveal: 0.25 });
		expect(isDrawing({ type: "graphic", definitionId: "drawing" } as never)).toBe(true);
		expect(isDrawing({ type: "graphic", definitionId: "star" } as never)).toBe(false);
		expect(brushFor("highlighter")).toEqual({ tool: "highlighter", color: "#ffeb3b", width: DRAW_TOOL_WIDTHS.highlighter });
	});
});

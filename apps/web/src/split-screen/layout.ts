/**
 * Split-screen layouts: 2, 3 or 4 clips side by side, stacked or in a grid,
 * in one step. Each layout is a set of cells on the canvas; each clip is
 * scaled to cover its cell (`transform.scaleX/Y` over the editor's
 * contain fit), moved to the cell's centre, and given a rectangle mask
 * cropped to the cell, so it fills the cell with nothing spilling over.
 * The same maths as `crates/editor/src/split_screen.rs` (both are checked
 * against `fixtures/split-screen/cases.json`).
 */

export type SplitLayoutId = "side-by-side" | "top-bottom" | "three-columns" | "three-rows" | "one-and-two" | "grid";

export interface SplitLayout {
	id: SplitLayoutId;
	label: string;
	/** Cells as shares of the canvas: x, y, width, height. */
	cells: readonly (readonly [number, number, number, number])[];
}

export const SPLIT_LAYOUTS: readonly SplitLayout[] = [
	{ id: "side-by-side", label: "Side by side", cells: [[0, 0, 0.5, 1], [0.5, 0, 0.5, 1]] },
	{ id: "top-bottom", label: "Top and bottom", cells: [[0, 0, 1, 0.5], [0, 0.5, 1, 0.5]] },
	{ id: "three-columns", label: "Three columns", cells: [[0, 0, 1 / 3, 1], [1 / 3, 0, 1 / 3, 1], [2 / 3, 0, 1 / 3, 1]] },
	{ id: "three-rows", label: "Three rows", cells: [[0, 0, 1, 1 / 3], [0, 1 / 3, 1, 1 / 3], [0, 2 / 3, 1, 1 / 3]] },
	{ id: "one-and-two", label: "One big, two small", cells: [[0, 0, 0.5, 1], [0.5, 0, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]] },
	{ id: "grid", label: "Grid of four", cells: [[0, 0, 0.5, 0.5], [0.5, 0, 0.5, 0.5], [0, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]] },
];

export function splitLayout(id: string): SplitLayout | undefined {
	return SPLIT_LAYOUTS.find((layout) => layout.id === id);
}

/** The layouts for a number of clips. */
export function layoutsFor(count: number): SplitLayout[] {
	return SPLIT_LAYOUTS.filter((layout) => layout.cells.length === count);
}

/** Where one clip goes: its transform and the rectangle mask (shares of its own frame). */
export interface SplitPlacement {
	positionX: number;
	positionY: number;
	scale: number;
	maskWidth: number;
	maskHeight: number;
}

/**
 * The placement of a clip of `sourceWidth` × `sourceHeight` in cell `cell`
 * of a `canvasWidth` × `canvasHeight` canvas, with `gap` canvas pixels
 * between cells (and none at the canvas edge).
 */
export function placeInCell({
	cell,
	canvasWidth,
	canvasHeight,
	sourceWidth,
	sourceHeight,
	gap = 0,
}: {
	cell: readonly [number, number, number, number];
	canvasWidth: number;
	canvasHeight: number;
	sourceWidth: number;
	sourceHeight: number;
	gap?: number;
}): SplitPlacement {
	const [x, y, w, h] = cell;
	// Half the gap comes off each inner edge.
	const half = Math.max(0, gap) / 2;
	const left = x * canvasWidth + (x > 1e-9 ? half : 0);
	const right = (x + w) * canvasWidth - (x + w < 1 - 1e-9 ? half : 0);
	const top = y * canvasHeight + (y > 1e-9 ? half : 0);
	const bottom = (y + h) * canvasHeight - (y + h < 1 - 1e-9 ? half : 0);
	const cellWidth = Math.max(1, right - left);
	const cellHeight = Math.max(1, bottom - top);
	const fit = Math.min(canvasWidth / sourceWidth, canvasHeight / sourceHeight);
	const fitWidth = sourceWidth * fit;
	const fitHeight = sourceHeight * fit;
	const scale = Math.max(cellWidth / fitWidth, cellHeight / fitHeight);
	return {
		positionX: (left + right) / 2 - canvasWidth / 2,
		positionY: (top + bottom) / 2 - canvasHeight / 2,
		scale,
		maskWidth: Math.min(1, cellWidth / (fitWidth * scale)),
		maskHeight: Math.min(1, cellHeight / (fitHeight * scale)),
	};
}

/** The placements of clips (in order) for a layout; the clip count must match its cells. */
export function planSplitScreen({
	layout,
	canvasWidth,
	canvasHeight,
	sources,
	gap = 0,
}: {
	layout: SplitLayout;
	canvasWidth: number;
	canvasHeight: number;
	sources: readonly { width: number; height: number }[];
	gap?: number;
}): SplitPlacement[] {
	if (sources.length !== layout.cells.length) {
		throw new Error(`${layout.label} needs ${layout.cells.length} clips; ${sources.length} are selected`);
	}
	return layout.cells.map((cell, index) =>
		placeInCell({
			cell,
			canvasWidth,
			canvasHeight,
			sourceWidth: Math.max(1, sources[index].width),
			sourceHeight: Math.max(1, sources[index].height),
			gap,
		}),
	);
}

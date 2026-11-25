import { drawingRasterSize, drawingRevealProgress, rasteriseDrawing } from "opencut-wasm";
import type { GraphicDefinition } from "../types";

/** The graphic definition of a freehand drawing (row 90); `editor::DRAWING_DEFINITION`. */
export const DRAWING_DEFINITION = "drawing";

/**
 * A freehand drawing over the whole canvas: its strokes (the `strokes`
 * param, not a field: drawn with Draw on the preview) rasterised by the
 * shared `strokes` crate, as the desktop does. `drawOn` plays the strokes
 * back as they were drawn over that many seconds (0 shows it whole).
 */
export const drawingGraphicDefinition: GraphicDefinition = {
	id: DRAWING_DEFINITION,
	name: "Drawing",
	keywords: ["drawing", "draw", "pen", "marker", "highlighter", "annotate", "freehand"],
	hidden: true,
	params: [
		{ key: "drawOn", label: "Draw on over (s)", type: "number", default: 0, min: 0, max: 600, step: 0.1, keyframable: false },
	],
	sourceSize: ({ width, height }) => {
		const [w, h] = drawingRasterSize(Math.max(1, Math.round(width)), Math.max(1, Math.round(height)));
		return { width: w, height: h };
	},
	timedParams: (params, elapsedSeconds) => ({
		__reveal: drawingRevealProgress(Number(params.drawOn) || 0, elapsedSeconds),
	}),
	render({ ctx, params, width, height }) {
		const strokes = typeof params.strokes === "string" ? params.strokes : "";
		if (!strokes) return;
		const reveal = typeof params.__reveal === "number" ? params.__reveal : 1;
		const rgba = rasteriseDrawing(strokes, width, height, reveal);
		if (rgba.length !== width * height * 4) return;
		ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
	},
};

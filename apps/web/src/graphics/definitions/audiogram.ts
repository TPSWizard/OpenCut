import { Audiogram, audiogramRasterSize } from "opencut-wasm";
import type { GraphicDefinition } from "../types";

/** The graphic definition of an audiogram (row 100); `audiogram::DEFINITION`. */
export const AUDIOGRAM_GRAPHIC = "audiogram";

/** Decoded levels by their stored text, so a frame draws without decoding them again. */
const decoded = new Map<string, Audiogram>();

function audiogramFor(levels: string): Audiogram {
	let found = decoded.get(levels);
	if (!found) {
		if (decoded.size >= 8) {
			const [oldest] = decoded.keys();
			decoded.get(oldest)?.free();
			decoded.delete(oldest);
		}
		found = new Audiogram(levels);
		decoded.set(levels, found);
	}
	return found;
}

const LOOK_KEYS = ["style", "color", "barCount", "size", "width", "y", "gap"] as const;

/**
 * An audiogram over the whole canvas: the sound levels in the `levels`
 * param (made with Make audiogram on a clip, not a field) drawn by the
 * shared `audiogram` crate at the time into the element, as the desktop
 * does. Sizes are percent of the canvas.
 */
export const audiogramGraphicDefinition: GraphicDefinition = {
	id: AUDIOGRAM_GRAPHIC,
	name: "Audiogram",
	keywords: ["audiogram", "waveform", "visualiser", "visualizer", "podcast", "bars", "spectrum"],
	hidden: true,
	params: [
		{
			key: "style",
			label: "Style",
			type: "select",
			default: "mirror",
			keyframable: false,
			options: [
				{ value: "bars", label: "Bars" },
				{ value: "mirror", label: "Mirrored bars" },
				{ value: "line", label: "Line" },
				{ value: "circle", label: "Circle" },
			],
		},
		{ key: "color", label: "Colour", type: "color", default: "#ffffff", keyframable: false },
		{ key: "barCount", label: "Bars", type: "number", default: 48, min: 4, max: 128, step: 1, keyframable: false },
		{ key: "size", label: "Height (% of canvas)", type: "number", default: 20, min: 1, max: 100, step: 1, keyframable: false },
		{ key: "width", label: "Width (% of canvas)", type: "number", default: 70, min: 5, max: 100, step: 1, keyframable: false },
		{ key: "y", label: "Position (% from top)", type: "number", default: 75, min: 0, max: 100, step: 1, keyframable: false },
		{ key: "gap", label: "Gap between bars (%)", type: "number", default: 30, min: 0, max: 90, step: 1, keyframable: false },
	],
	sourceSize: ({ width, height }) => {
		const [w, h] = audiogramRasterSize(Math.max(1, Math.round(width)), Math.max(1, Math.round(height)));
		return { width: w, height: h };
	},
	timedParams: (_params, elapsedSeconds) => ({ __seconds: elapsedSeconds }),
	render({ ctx, params, width, height }) {
		const levels = typeof params.levels === "string" ? params.levels : "";
		if (!levels) return;
		const look: Record<string, unknown> = {};
		for (const key of LOOK_KEYS) look[key] = params[key];
		const seconds = typeof params.__seconds === "number" ? params.__seconds : 0;
		const rgba = audiogramFor(levels).render(JSON.stringify(look), seconds, width, height);
		if (rgba.length !== width * height * 4) return;
		ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
	},
};

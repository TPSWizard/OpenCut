// jsdom ships no <canvas> implementation. The text layout code only needs a
// 2D context to measure strings, so provide a deterministic stand-in that
// approximates glyph widths from the font size. Rendering calls are no-ops.

type Metrics = {
	width: number;
	actualBoundingBoxLeft: number;
	actualBoundingBoxRight: number;
	actualBoundingBoxAscent: number;
	actualBoundingBoxDescent: number;
	fontBoundingBoxAscent: number;
	fontBoundingBoxDescent: number;
	emHeightAscent: number;
	emHeightDescent: number;
	alphabeticBaseline: number;
	hangingBaseline: number;
	ideographicBaseline: number;
};

const FONT_SIZE_PATTERN = /(\d+(?:\.\d+)?)px/;
const AVERAGE_GLYPH_WIDTH = 0.6;

function createMeasurementContext(): CanvasRenderingContext2D {
	const state = { font: "16px sans-serif" };
	const noop = () => {};
	const context = {
		get font() {
			return state.font;
		},
		set font(value: string) {
			state.font = value;
		},
		textAlign: "left",
		textBaseline: "alphabetic",
		fillStyle: "#000",
		strokeStyle: "#000",
		lineWidth: 1,
		lineJoin: "miter",
		lineCap: "butt",
		letterSpacing: "0px",
		measureText(text: string): Metrics {
			const fontSize = Number(FONT_SIZE_PATTERN.exec(state.font)?.[1] ?? 16);
			const width = text.length * fontSize * AVERAGE_GLYPH_WIDTH;
			const ascent = fontSize * 0.8;
			const descent = fontSize * 0.2;
			return {
				width,
				actualBoundingBoxLeft: 0,
				actualBoundingBoxRight: width,
				actualBoundingBoxAscent: ascent,
				actualBoundingBoxDescent: descent,
				fontBoundingBoxAscent: ascent,
				fontBoundingBoxDescent: descent,
				emHeightAscent: ascent,
				emHeightDescent: descent,
				alphabeticBaseline: 0,
				hangingBaseline: ascent,
				ideographicBaseline: -descent,
			};
		},
		save: noop,
		restore: noop,
		beginPath: noop,
		closePath: noop,
		fill: noop,
		stroke: noop,
		fillText: noop,
		strokeText: noop,
		fillRect: noop,
		clearRect: noop,
		roundRect: noop,
		rect: noop,
		moveTo: noop,
		lineTo: noop,
		translate: noop,
		scale: noop,
		rotate: noop,
		setTransform: noop,
		resetTransform: noop,
	};
	return context as unknown as CanvasRenderingContext2D;
}

if (typeof HTMLCanvasElement !== "undefined") {
	const original = HTMLCanvasElement.prototype.getContext;
	HTMLCanvasElement.prototype.getContext = function getContext(
		this: HTMLCanvasElement,
		contextId: string,
		...rest: unknown[]
	) {
		if (contextId === "2d") {
			return createMeasurementContext();
		}
		return (original as (...args: unknown[]) => unknown).call(this, contextId, ...rest);
	} as typeof HTMLCanvasElement.prototype.getContext;
}

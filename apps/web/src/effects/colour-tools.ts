/**
 * Colour tools both editors draw controls for: the HSL effect's eight hue
 * bands (whose values are also the hue curves) and colour wheels over lift
 * / gamma / gain. The same maths as the desktop's `editor::colour_tools`,
 * checked against `fixtures/hsl/cases.json`.
 */

/** The HSL effect's bands: key prefix, label and centre hue (degrees). */
export const HSL_BANDS = [
	{ key: "red", label: "Red", hue: 0 },
	{ key: "orange", label: "Orange", hue: 30 },
	{ key: "yellow", label: "Yellow", hue: 60 },
	{ key: "green", label: "Green", hue: 120 },
	{ key: "aqua", label: "Aqua", hue: 180 },
	{ key: "blue", label: "Blue", hue: 240 },
	{ key: "purple", label: "Purple", hue: 270 },
	{ key: "magenta", label: "Magenta", hue: 300 },
] as const;

/** What each band moves, as the HSL param key suffixes. */
export const HSL_CONTROLS = ["Hue", "Saturation", "Lightness"] as const;

/** The HSL param key of a band's control, e.g. `redHue`, `aquaLightness`. */
export function hslParamKey(band: number, control: number): string {
	return `${HSL_BANDS[band].key}${HSL_CONTROLS[control]}`;
}

/**
 * A control's value at `hue` degrees from its eight band values: the two
 * neighbouring bands blended with a smoothstep, as the shader does (the
 * hue curve the panels draw).
 */
export function hslBandValue(values: readonly number[], hue: number): number {
	const h = ((hue % 360) + 360) % 360;
	const centre = (index: number) => (index === 8 ? 360 : HSL_BANDS[index].hue);
	let index = 7;
	for (let k = 0; k < 7; k++) {
		if (h >= centre(k) && h < centre(k + 1)) index = k;
	}
	const t = Math.min(Math.max((h - centre(index)) / (centre(index + 1) - centre(index)), 0), 1);
	const w = t * t * (3 - 2 * t);
	return values[index] * (1 - w) + values[(index + 1) % 8] * w;
}

/** How far a wheel's rim moves a channel's lift, gamma or gain offset. */
export const COLOUR_WHEEL_RANGE = 0.5;
/** Where red, green and blue sit on a wheel (degrees anticlockwise from the right). */
export const COLOUR_WHEEL_ANGLES = [105, 225, 345] as const;

/** The lift / gamma / gain params a wheel moves. */
export const COLOUR_WHEELS = [
	{ label: "Lift", keys: ["liftRed", "liftGreen", "liftBlue"] },
	{ label: "Gamma", keys: ["gammaRed", "gammaGreen", "gammaBlue"] },
	{ label: "Gain", keys: ["gainRed", "gainGreen", "gainBlue"] },
] as const;

const radians = (degrees: number) => (degrees * Math.PI) / 180;

/**
 * Red, green and blue offsets of a dot at (x, y) in the unit disc (y up;
 * outside is pulled onto the rim). They sum to zero: the wheel tints
 * without brightening.
 */
export function colourWheelOffsets(x: number, y: number): [number, number, number] {
	const magnitude = Math.min(Math.hypot(x, y), 1);
	const theta = Math.atan2(y, x);
	const [r, g, b] = COLOUR_WHEEL_ANGLES.map((angle) => magnitude * Math.cos(theta - radians(angle)) * COLOUR_WHEEL_RANGE);
	return [r, g, b];
}

/** Where a wheel's dot sits for these offsets (their shared part is left out). */
export function colourWheelPoint(offsets: readonly number[]): { x: number; y: number } {
	const mean = (offsets[0] + offsets[1] + offsets[2]) / 3;
	let x = 0;
	let y = 0;
	COLOUR_WHEEL_ANGLES.forEach((angle, channel) => {
		x += (offsets[channel] - mean) * Math.cos(radians(angle));
		y += (offsets[channel] - mean) * Math.sin(radians(angle));
	});
	return { x: ((2 / 3) * x) / COLOUR_WHEEL_RANGE, y: ((2 / 3) * y) / COLOUR_WHEEL_RANGE };
}

/** New offsets for a dot moved to (x, y), keeping the old offsets' shared part. */
export function colourWheelMove(old: readonly number[], x: number, y: number): [number, number, number] {
	const mean = (old[0] + old[1] + old[2]) / 3;
	const [r, g, b] = colourWheelOffsets(x, y);
	return [r + mean, g + mean, b + mean];
}

/** A `size` x `size` RGBA picture of a wheel (mid grey tinted as a dot there would tint). */
export function colourWheelRgba(size: number): Uint8ClampedArray {
	const rgba = new Uint8ClampedArray(size * size * 4);
	const half = size / 2;
	for (let row = 0; row < size; row++) {
		for (let column = 0; column < size; column++) {
			const x = (column + 0.5 - half) / half;
			const y = (half - row - 0.5) / half;
			const distance = Math.hypot(x, y);
			if (distance > 1) continue;
			const offsets = colourWheelOffsets(x, y);
			const index = (row * size + column) * 4;
			for (let channel = 0; channel < 3; channel++) {
				rgba[index + channel] = Math.round(Math.min(Math.max(0.5 + offsets[channel], 0), 1) * 255);
			}
			rgba[index + 3] = Math.round(Math.min(Math.max((1 - distance) * half, 0), 1) * 255);
		}
	}
	return rgba;
}

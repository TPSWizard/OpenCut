import type { ParamValues } from "@/params";

/**
 * Rounded corners, an outline and a drop shadow (or glow) for a visual
 * clip, from its params: `cornerRadius` (percent of half the shorter side;
 * not on text, whose box has its own), `outline.width` / `outline.color`,
 * and `shadow.enabled` / `shadow.color` / `shadow.opacity` / `shadow.blur`
 * / `shadow.offsetX` / `shadow.offsetY`. Lengths are canvas pixels. The
 * shared compositor draws them (`LayerStyleDescriptor` in
 * `crates/compositor`); the desktop's `editor::LayerStyle` reads the same
 * params.
 */

export const CORNER_RADIUS_MAX = 100;
export const OUTLINE_WIDTH_MAX = 50;
export const SHADOW_BLUR_MAX = 100;
export const SHADOW_OFFSET_MAX = 500;
export const SHADOW_OPACITY_DEFAULT = 0.6;
export const SHADOW_BLUR_DEFAULT = 16;
export const SHADOW_OFFSET_DEFAULT = 8;

export type Rgba = [number, number, number, number];

export interface LayerStyle {
	/** Percent (0–100) of half the clip's shorter side. */
	cornerRadius: number;
	outline: { width: number; color: Rgba } | null;
	/** Colour with the opacity folded into alpha. */
	shadow: { color: Rgba; offsetX: number; offsetY: number; blur: number } | null;
}

/** The compositor's `LayerStyleDescriptor`, in frame pixels. */
export interface LayerStyleDescriptor {
	cornerRadius: number;
	outline: { width: number; color: Rgba } | null;
	shadow: { color: Rgba; offsetX: number; offsetY: number; blur: number } | null;
}

/** `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` as straight RGBA from 0 to 1, or null. */
export function parseHexColor(text: unknown): Rgba | null {
	if (typeof text !== "string") return null;
	const hex = text.trim().replace(/^#/, "");
	if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
	let digits: number[];
	if (hex.length === 3 || hex.length === 4) {
		digits = [...hex].map((digit) => Number.parseInt(digit, 16) * 17);
	} else if (hex.length === 6 || hex.length === 8) {
		digits = hex.match(/../g)!.map((pair) => Number.parseInt(pair, 16));
	} else {
		return null;
	}
	const [r, g, b, a = 255] = digits;
	return [r / 255, g / 255, b / 255, a / 255];
}

function readNumber(value: unknown, fallback: number, min: number, max: number): number {
	const number = typeof value === "number" && Number.isFinite(value) ? value : fallback;
	return Math.min(max, Math.max(min, number));
}

/** The clip's style, or null when it has none. */
export function readLayerStyle(params: ParamValues | undefined, { isText = false } = {}): LayerStyle | null {
	if (!params) return null;
	const cornerRadius = isText ? 0 : readNumber(params.cornerRadius, 0, 0, CORNER_RADIUS_MAX);
	const outlineWidth = readNumber(params["outline.width"], 0, 0, OUTLINE_WIDTH_MAX);
	const outline =
		outlineWidth > 0 ? { width: outlineWidth, color: parseHexColor(params["outline.color"]) ?? [1, 1, 1, 1] } : null;
	let shadow: LayerStyle["shadow"] = null;
	if (params["shadow.enabled"] === true) {
		const color = parseHexColor(params["shadow.color"]) ?? [0, 0, 0, 1];
		color[3] *= readNumber(params["shadow.opacity"], SHADOW_OPACITY_DEFAULT, 0, 1);
		if (color[3] > 0) {
			shadow = {
				color,
				offsetX: readNumber(params["shadow.offsetX"], SHADOW_OFFSET_DEFAULT, -SHADOW_OFFSET_MAX, SHADOW_OFFSET_MAX),
				offsetY: readNumber(params["shadow.offsetY"], SHADOW_OFFSET_DEFAULT, -SHADOW_OFFSET_MAX, SHADOW_OFFSET_MAX),
				blur: readNumber(params["shadow.blur"], SHADOW_BLUR_DEFAULT, 0, SHADOW_BLUR_MAX),
			};
		}
	}
	if (cornerRadius <= 0 && !outline && !shadow) return null;
	return { cornerRadius, outline, shadow };
}

/**
 * The style for the compositor, for a quad `width` by `height` frame
 * pixels; `scale` takes canvas pixels to frame pixels.
 */
export function layerStyleDescriptor({
	style,
	width,
	height,
	scale = 1,
}: {
	style: LayerStyle;
	width: number;
	height: number;
	scale?: number;
}): LayerStyleDescriptor {
	return {
		cornerRadius: (style.cornerRadius / 100) * (Math.min(Math.abs(width), Math.abs(height)) / 2),
		outline: style.outline ? { width: style.outline.width * scale, color: style.outline.color } : null,
		shadow: style.shadow
			? {
					color: style.shadow.color,
					offsetX: style.shadow.offsetX * scale,
					offsetY: style.shadow.offsetY * scale,
					blur: style.shadow.blur * scale,
				}
			: null,
	};
}

/** One-click looks for the Style tab. */
export const SHADOW_PRESETS: Record<"shadow" | "glow", ParamValues> = {
	shadow: {
		"shadow.enabled": true,
		"shadow.color": "#000000",
		"shadow.opacity": SHADOW_OPACITY_DEFAULT,
		"shadow.blur": SHADOW_BLUR_DEFAULT,
		"shadow.offsetX": SHADOW_OFFSET_DEFAULT,
		"shadow.offsetY": SHADOW_OFFSET_DEFAULT,
	},
	glow: {
		"shadow.enabled": true,
		"shadow.color": "#ffffff",
		"shadow.opacity": 0.9,
		"shadow.blur": 24,
		"shadow.offsetX": 0,
		"shadow.offsetY": 0,
	},
};

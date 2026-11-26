/**
 * Word art (row 99): gradient fill, up to three stacked outlines and an
 * inner shadow on a text clip's glyphs, and a gallery of styles (our own
 * designs). The glyphs are drawn off-screen and styled by the shared Rust
 * `wordart` crate through WASM, as the desktop renderer styles its glyph
 * raster, so a style looks the same in both editors. The params are flat
 * text params (`wordArt.fill`, `wordArt.stroke1Width`, ...); widths,
 * distances and blurs are percent of the font size. Checked against
 * `fixtures/word-art/cases.json`.
 */

import { wordArtCompose, wordArtOutset, wordArtStyleParams, wordArtStyles } from "opencut-wasm";
import type { EditorCore } from "@/core";
import type { TextElement } from "@/timeline";

/** Every word-art param, as the crate names them (`wordart::keys`). */
export const WORD_ART_KEYS = [
	"wordArt.style",
	"wordArt.fill",
	"wordArt.gradientFrom",
	"wordArt.gradientVia",
	"wordArt.gradientTo",
	"wordArt.gradientAngle",
	"wordArt.stroke1Color",
	"wordArt.stroke1Width",
	"wordArt.stroke2Color",
	"wordArt.stroke2Width",
	"wordArt.stroke3Color",
	"wordArt.stroke3Width",
	"wordArt.innerShadowOpacity",
	"wordArt.innerShadowColor",
	"wordArt.innerShadowDistance",
	"wordArt.innerShadowAngle",
	"wordArt.innerShadowBlur",
] as const;

export interface WordArtStyle {
	id: string;
	label: string;
	description: string;
	params: Record<string, string | number>;
}

let styles: WordArtStyle[] | null = null;

/** The gallery, in order (the first is "Plain"). */
export function getWordArtStyles(): WordArtStyle[] {
	styles ??= JSON.parse(wordArtStyles()) as WordArtStyle[];
	return styles;
}

/** The params a gallery style sets (every word-art param, maybe `color`), or null for an unknown id. */
export function wordArtStyleParamsFor(id: string): Record<string, string | number> | null {
	return JSON.parse(wordArtStyleParams(id));
}

const number = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);

/**
 * The clip's word-art params as JSON for the crate, or null when it has
 * none (plain text skips the off-screen pass).
 */
export function wordArtParamsJson(params: Record<string, unknown> | undefined): string | null {
	if (!params) return null;
	const active =
		params["wordArt.fill"] === "gradient" ||
		number(params["wordArt.stroke1Width"]) > 0 ||
		number(params["wordArt.stroke2Width"]) > 0 ||
		number(params["wordArt.stroke3Width"]) > 0 ||
		number(params["wordArt.innerShadowOpacity"]) > 0;
	if (!active) return null;
	const picked: Record<string, unknown> = {};
	for (const key of WORD_ART_KEYS) {
		if (params[key] !== undefined) picked[key] = params[key];
	}
	return JSON.stringify(picked);
}

/** Pixels the outlines reach past the glyphs for a font `emPx` tall (0 for none). */
export function wordArtOutsetPx(json: string | null, emPx: number): number {
	return json ? wordArtOutset(json, emPx) : 0;
}

type Context2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

function makeCanvas(width: number, height: number): OffscreenCanvas | HTMLCanvasElement {
	if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(width, height);
	const canvas = document.createElement("canvas");
	canvas.width = width;
	canvas.height = height;
	return canvas;
}

/** The most device pixels a styled glyph raster may have on a side. */
const MAX_SIDE = 8192;

/**
 * Draws the glyphs styled by word art into `ctx` (already moved to the
 * text's anchor, scaled and turned). `rect` holds the glyphs and their
 * outlines in text units; `emPx` is the font size in text units;
 * `drawGlyphs` draws the glyphs (no background box) there. They are drawn
 * off-screen at the device pixel size the context shows them at, styled,
 * and drawn back.
 */
export function drawWordArtGlyphs({
	ctx,
	json,
	rect,
	emPx,
	drawGlyphs,
}: {
	ctx: Context2D;
	json: string;
	rect: { left: number; top: number; width: number; height: number };
	emPx: number;
	drawGlyphs: (ctx: Context2D) => void;
}): void {
	const matrix = ctx.getTransform();
	const deviceScale = Math.max(Math.hypot(matrix.a, matrix.b), Math.hypot(matrix.c, matrix.d));
	const pad = 2;
	let scale = Math.min(8, Math.max(0.05, deviceScale || 1));
	const longest = Math.max(rect.width, rect.height) + 2 * pad;
	if (longest * scale > MAX_SIDE) scale = MAX_SIDE / longest;
	const width = Math.max(1, Math.ceil((rect.width + 2 * pad) * scale));
	const height = Math.max(1, Math.ceil((rect.height + 2 * pad) * scale));
	const left = rect.left - pad;
	const top = rect.top - pad;

	const layer = makeCanvas(width, height);
	const layerCtx = layer.getContext("2d") as Context2D | null;
	if (!layerCtx) return;
	layerCtx.setTransform(scale, 0, 0, scale, -left * scale, -top * scale);
	drawGlyphs(layerCtx);
	const pixels = layerCtx.getImageData(0, 0, width, height);
	const styled = wordArtCompose(json, width, height, new Uint8Array(pixels.data.buffer), emPx * scale);
	if (styled.length === width * height * 4) {
		const data = new Uint8ClampedArray(styled.length);
		data.set(styled);
		layerCtx.setTransform(1, 0, 0, 1, 0, 0);
		layerCtx.putImageData(new ImageData(data, width, height), 0, 0);
	}
	ctx.drawImage(layer, left, top, width / scale, height / scale);
}

/** Puts a gallery style on text clips (one undo step); returns how many changed. */
export function applyWordArtStyle({
	editor,
	targets,
	id,
}: {
	editor: EditorCore;
	targets: Array<{ trackId: string; element: TextElement }>;
	id: string;
}): number {
	const values = wordArtStyleParamsFor(id);
	if (!values) throw new Error(`Unknown word-art style ${id}`);
	if (targets.length === 0) return 0;
	editor.timeline.updateElements({
		updates: targets.map(({ trackId, element }) => ({
			trackId,
			elementId: element.id,
			patch: { params: { ...element.params, ...values } } as never,
		})),
	});
	return targets.length;
}

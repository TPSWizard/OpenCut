import { BaseNode } from "./base-node";
import type { TextElement } from "@/timeline";
import type { EffectPass } from "@/effects/types";
import type { BlendMode, Transform } from "@/rendering";
import {
	drawMeasuredTextLayout,
	type TextKeywords,
	type TextWordHighlight,
} from "@/text/primitives";
import type { MeasuredTextElement } from "@/text/measure-element";
import { drawCurvedText, isCurved } from "@/text/curve";
import { drawWordArtGlyphs } from "@/text/word-art";
import type { MotionTransforms } from "@/motion-blur/blur";
import type { RichCharStyle } from "@/text/rich";

export type TextNodeParams = TextElement & {
	transform: Transform;
	opacity: number;
	blendMode?: BlendMode;
	canvasCenter: { x: number; y: number };
	canvasHeight: number;
	textBaseline?: CanvasTextBaseline;
};

export interface ResolvedTextNodeState {
	transform: Transform;
	opacity: number;
	textColor: string;
	backgroundColor: string;
	effectPasses: EffectPass[][];
	measuredText: MeasuredTextElement;
	/** The caption words to emphasise at this time, if the element has words. */
	highlight: TextWordHighlight | null;
	/** The caption's keywords and their look, if any word is one. */
	keywords: TextKeywords | null;
	/** Characters shown by a typewriter or word-by-word animation (newlines not counted), or null for all. */
	revealChars: number | null;
	/** Curved text at this time (`@/text/curve`): -100 to 100, 0 flat. */
	curve?: number;
	/** Text on a path (`textPathAt`): control points, or null. */
	path?: Float64Array | null;
	/** The transform at shutter open and close, when motion blurred. */
	motion?: MotionTransforms | null;
	/** Each character's rich-text style (`@/text/rich`); null for none. */
	rich?: RichCharStyle[] | null;
	/** The clip's word-art params as JSON (`@/text/word-art`); null for plain text. */
	wordArt?: string | null;
}

export class TextNode extends BaseNode<TextNodeParams, ResolvedTextNodeState> {}

export function renderTextToContext({
	node,
	ctx,
}: {
	node: TextNode;
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
}): void {
	const resolved = node.resolved;
	if (!resolved) {
		return;
	}

	const x = resolved.transform.position.x + node.params.canvasCenter.x;
	const y = resolved.transform.position.y + node.params.canvasCenter.y;
	const baseline = node.params.textBaseline ?? "middle";

	ctx.save();
	ctx.translate(x, y);
	ctx.scale(resolved.transform.scaleX, resolved.transform.scaleY);
	if (resolved.transform.rotate) {
		ctx.rotate((resolved.transform.rotate * Math.PI) / 180);
	}

	const drawText = (
		target: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
		part: "all" | "background" | "glyphs",
	) =>
		drawMeasuredTextLayout({
			ctx: target,
			layout: resolved.measuredText,
			textColor: resolved.textColor,
			background: part === "glyphs" ? null : resolved.measuredText.resolvedBackground,
			backgroundColor: resolved.backgroundColor,
			textBaseline: baseline,
			highlight: resolved.highlight,
			keywords: resolved.keywords,
			revealChars: resolved.revealChars,
			rich: resolved.rich ?? null,
			backgroundOnly: part === "background",
		});
	// Word art (row 99): the box as usual, then the glyphs styled off-screen.
	const wordArt = resolved.wordArt;
	const drawFlat = (target: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D) => {
		if (!wordArt) {
			drawText(target, "all");
			return;
		}
		drawText(target, "background");
		drawWordArtGlyphs({
			ctx: target,
			json: wordArt,
			rect: resolved.measuredText.visualRect,
			emPx: resolved.measuredText.scaledFontSize,
			drawGlyphs: (layer) => drawText(layer, "glyphs"),
		});
	};
	if (resolved.path || (resolved.curve !== undefined && isCurved(resolved.curve))) {
		drawCurvedText({ ctx, curve: resolved.curve ?? 0, path: resolved.path, rect: resolved.measuredText.visualRect, drawFlat });
	} else {
		drawFlat(ctx);
	}

	ctx.restore();
}

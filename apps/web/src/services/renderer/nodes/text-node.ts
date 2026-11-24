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
import type { MotionTransforms } from "@/motion-blur/blur";

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
	/** The transform at shutter open and close, when motion blurred. */
	motion?: MotionTransforms | null;
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

	const drawFlat = (target: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D) =>
		drawMeasuredTextLayout({
			ctx: target,
			layout: resolved.measuredText,
			textColor: resolved.textColor,
			background: resolved.measuredText.resolvedBackground,
			backgroundColor: resolved.backgroundColor,
			textBaseline: baseline,
			highlight: resolved.highlight,
			keywords: resolved.keywords,
			revealChars: resolved.revealChars,
		});
	if (resolved.curve !== undefined && isCurved(resolved.curve)) {
		drawCurvedText({ ctx, curve: resolved.curve, rect: resolved.measuredText.visualRect, drawFlat });
	} else {
		drawFlat(ctx);
	}

	ctx.restore();
}

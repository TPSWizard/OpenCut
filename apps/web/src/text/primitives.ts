import type { TextCanvasContext, TextBlockMeasurement } from "@/text/layout";
import { DEFAULTS } from "@/timeline/defaults";
import { clamp } from "@/utils/math";
import { CORNER_RADIUS_MAX, CORNER_RADIUS_MIN } from "./background";
import {
	drawTextDecoration,
	getTextBackgroundRect,
	measureTextBlock,
	setCanvasLetterSpacing,
} from "./layout";
import { FONT_SIZE_SCALE_REFERENCE } from "./typography";
import { placeWords } from "@/captions/word-layout";
import type { CaptionHighlight } from "@/captions/types";

export type TextAlign = "left" | "center" | "right";
export type TextFontWeight = "normal" | "bold";
export type TextFontStyle = "normal" | "italic";
export type TextDecoration = "none" | "underline" | "line-through";

export interface TextLayoutParams {
	content: string;
	fontSize: number;
	fontFamily: string;
	fontWeight: TextFontWeight;
	fontStyle: TextFontStyle;
	textAlign: TextAlign;
	textDecoration?: TextDecoration;
	letterSpacing?: number;
	lineHeight?: number;
}

export interface ResolvedTextLayout {
	scaledFontSize: number;
	fontString: string;
	letterSpacing: number;
	lineHeightPx: number;
	fontSizeRatio: number;
	textAlign: TextAlign;
	textDecoration: TextDecoration;
}

export interface MeasuredTextLayout extends ResolvedTextLayout {
	lines: string[];
	lineMetrics: TextMetrics[];
	block: TextBlockMeasurement;
}

/**
 * Which words of the block stand out and how; see `CaptionHighlight`. The
 * words are the block's whitespace-separated tokens in order.
 */
export interface TextWordHighlight {
	mode: CaptionHighlight;
	activeColor: string;
	boxColor: string;
	/** Index of the word being spoken, or -1 between words. */
	activeIndex: number;
	/** Number of words spoken so far, the active one included. */
	spokenCount: number;
}

const HIGHLIGHT_BOX_PADDING_X = 0.18;
const HIGHLIGHT_BOX_PADDING_Y = 0.08;
const HIGHLIGHT_BOX_HEIGHT = 1.1;
const HIGHLIGHT_BOX_RADIUS = 0.2;

export interface ResolvedTextBackgroundLike {
	enabled: boolean;
	color: string;
	paddingX: number;
	paddingY: number;
	offsetX: number;
	offsetY: number;
	cornerRadius: number;
}

export function quoteFontFamily({ fontFamily }: { fontFamily: string }): string {
	return `"${fontFamily.replace(/"/g, '\\"')}"`;
}

export function buildTextFontString({
	fontFamily,
	fontWeight,
	fontStyle,
	scaledFontSize,
}: {
	fontFamily: string;
	fontWeight: TextFontWeight;
	fontStyle: TextFontStyle;
	scaledFontSize: number;
}): string {
	return `${fontStyle} ${fontWeight} ${scaledFontSize}px ${quoteFontFamily({ fontFamily })}, sans-serif`;
}

export function resolveTextLayout({
	text,
	canvasHeight,
}: {
	text: TextLayoutParams;
	canvasHeight: number;
}): ResolvedTextLayout {
	const scaledFontSize =
		text.fontSize * (canvasHeight / FONT_SIZE_SCALE_REFERENCE);
	const fontWeight = text.fontWeight === "bold" ? "bold" : "normal";
	const fontStyle = text.fontStyle === "italic" ? "italic" : "normal";
	const letterSpacing = text.letterSpacing ?? DEFAULTS.text.letterSpacing;
	const lineHeightPx =
		scaledFontSize * (text.lineHeight ?? DEFAULTS.text.lineHeight);
	const fontSizeRatio = text.fontSize / 15;

	return {
		scaledFontSize,
		fontString: buildTextFontString({
			fontFamily: text.fontFamily,
			fontWeight,
			fontStyle,
			scaledFontSize,
		}),
		letterSpacing,
		lineHeightPx,
		fontSizeRatio,
		textAlign: text.textAlign,
		textDecoration: text.textDecoration ?? "none",
	};
}

export function measureTextLayout({
	text,
	canvasHeight,
	ctx,
}: {
	text: TextLayoutParams;
	canvasHeight: number;
	ctx: TextCanvasContext;
}): MeasuredTextLayout {
	const resolvedLayout = resolveTextLayout({ text, canvasHeight });
	const lines = text.content.split("\n");

	ctx.save();
	ctx.font = resolvedLayout.fontString;
	ctx.textBaseline = "middle";
	setCanvasLetterSpacing({
		ctx,
		letterSpacingPx: resolvedLayout.letterSpacing,
	});
	const lineMetrics = lines.map((line) => ctx.measureText(line));
	ctx.restore();

	const block = measureTextBlock({
		lineMetrics,
		lineHeightPx: resolvedLayout.lineHeightPx,
	});

	return {
		...resolvedLayout,
		lines,
		lineMetrics,
		block,
	};
}

export function drawMeasuredTextLayout({
	ctx,
	layout,
	textColor,
	background,
	backgroundColor,
	textBaseline = "middle",
	highlight,
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	textColor: string;
	background?: ResolvedTextBackgroundLike | null;
	backgroundColor?: string;
	textBaseline?: CanvasTextBaseline;
	highlight?: TextWordHighlight | null;
}): void {
	ctx.font = layout.fontString;
	ctx.textAlign = layout.textAlign;
	ctx.textBaseline = textBaseline;
	ctx.fillStyle = textColor;
	setCanvasLetterSpacing({ ctx, letterSpacingPx: layout.letterSpacing });

	if (
		background?.enabled &&
		backgroundColor &&
		backgroundColor !== "transparent" &&
		layout.lines.length > 0
	) {
		const backgroundRect = getTextBackgroundRect({
			textAlign: layout.textAlign,
			block: layout.block,
			background: {
				...background,
				color: backgroundColor,
			},
			fontSizeRatio: layout.fontSizeRatio,
		});
		if (backgroundRect) {
			const p =
				clamp({
					value: background.cornerRadius,
					min: CORNER_RADIUS_MIN,
					max: CORNER_RADIUS_MAX,
				}) / 100;
			const radius =
				(Math.min(backgroundRect.width, backgroundRect.height) / 2) * p;
			ctx.fillStyle = backgroundColor;
			ctx.beginPath();
			ctx.roundRect(
				backgroundRect.left,
				backgroundRect.top,
				backgroundRect.width,
				backgroundRect.height,
				radius,
			);
			ctx.fill();
			ctx.fillStyle = textColor;
		}
	}

	for (let index = 0; index < layout.lines.length; index++) {
		const lineY = index * layout.lineHeightPx - layout.block.visualCenterOffset;
		ctx.fillText(layout.lines[index], 0, lineY);
		drawTextDecoration({
			ctx,
			textDecoration: layout.textDecoration,
			lineWidth: layout.lineMetrics[index].width,
			lineY,
			metrics: layout.lineMetrics[index],
			scaledFontSize: layout.scaledFontSize,
			textAlign: layout.textAlign,
		});
	}

	if (highlight && highlight.mode !== "none") {
		drawHighlightedWords({ ctx, layout, highlight, textColor });
	}
}

/** The indices of the words `highlight` singles out. */
export function highlightedWordIndices({
	highlight,
}: {
	highlight: TextWordHighlight;
}): number[] {
	switch (highlight.mode) {
		case "none":
			return [];
		case "word":
		case "box":
			return highlight.activeIndex >= 0 ? [highlight.activeIndex] : [];
		case "fill":
			return Array.from({ length: Math.max(0, highlight.spokenCount) }, (_, index) => index);
	}
}

/**
 * Redraws the highlighted words over the plain lines: each word is found
 * by its prefix on its line, so the wrapping does not matter. A box, when
 * asked for, is a rounded rectangle one em tall behind the active word.
 */
function drawHighlightedWords({
	ctx,
	layout,
	highlight,
	textColor,
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	highlight: TextWordHighlight;
	textColor: string;
}): void {
	const wanted = new Set(highlightedWordIndices({ highlight }));
	if (wanted.size === 0) {
		return;
	}
	const placements = placeWords({ lines: layout.lines }).filter((placement) =>
		wanted.has(placement.index),
	);
	if (placements.length === 0) {
		return;
	}
	const em = layout.scaledFontSize;
	ctx.save();
	ctx.textAlign = "left";
	for (const placement of placements) {
		const lineWidth = layout.lineMetrics[placement.line].width;
		const prefixWidth = ctx.measureText(placement.prefix).width;
		const wordWidth = ctx.measureText(placement.text).width;
		const lineY = placement.line * layout.lineHeightPx - layout.block.visualCenterOffset;
		let x = prefixWidth;
		if (layout.textAlign === "center") {
			x -= lineWidth / 2;
		} else if (layout.textAlign === "right") {
			x -= lineWidth;
		}
		if (highlight.mode === "box") {
			const padX = em * HIGHLIGHT_BOX_PADDING_X;
			const padY = em * HIGHLIGHT_BOX_PADDING_Y;
			const height = em * HIGHLIGHT_BOX_HEIGHT + 2 * padY;
			ctx.fillStyle = highlight.boxColor;
			ctx.beginPath();
			ctx.roundRect(
				x - padX,
				lineY - height / 2,
				wordWidth + 2 * padX,
				height,
				em * HIGHLIGHT_BOX_RADIUS,
			);
			ctx.fill();
		}
		ctx.fillStyle = highlight.activeColor;
		ctx.fillText(placement.text, x, lineY);
	}
	ctx.fillStyle = textColor;
	ctx.restore();
}

export function strokeMeasuredTextLayout({
	ctx,
	layout,
	strokeColor,
	strokeWidth,
	textBaseline = "middle",
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	strokeColor: string;
	strokeWidth: number;
	textBaseline?: CanvasTextBaseline;
}): void {
	ctx.font = layout.fontString;
	ctx.textAlign = layout.textAlign;
	ctx.textBaseline = textBaseline;
	ctx.strokeStyle = strokeColor;
	ctx.lineWidth = strokeWidth;
	ctx.lineJoin = "round";
	ctx.lineCap = "round";
	setCanvasLetterSpacing({ ctx, letterSpacingPx: layout.letterSpacing });

	for (let index = 0; index < layout.lines.length; index++) {
		const lineY = index * layout.lineHeightPx - layout.block.visualCenterOffset;
		ctx.strokeText(layout.lines[index], 0, lineY);
	}
}

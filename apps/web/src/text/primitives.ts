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
import type { RichCharStyle } from "./rich";

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

/**
 * A caption's keywords (by word index) and how they look: the keyword
 * colour (the spoken-word highlight still wins) and a size relative to the
 * other words; the line keeps its height. The desktop draws the same
 * (`editor::TextKeywords`).
 */
export interface TextKeywords {
	indices: number[];
	color: string;
	scale: number;
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
	keywords = null,
	revealChars = null,
	rich = null,
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	textColor: string;
	background?: ResolvedTextBackgroundLike | null;
	backgroundColor?: string;
	textBaseline?: CanvasTextBaseline;
	highlight?: TextWordHighlight | null;
	/** Caption keywords to draw in their colour and size; null for none. */
	keywords?: TextKeywords | null;
	/**
	 * Draw only this many characters (newlines not counted), each where it
	 * sits in the whole text, as a typewriter animation reveals it; null
	 * draws everything.
	 */
	revealChars?: number | null;
	/** Each character's rich-text style (`@/text/rich`); null for none. */
	rich?: RichCharStyle[] | null;
}): void {
	ctx.font = layout.fontString;
	ctx.textAlign = layout.textAlign;
	ctx.textBaseline = textBaseline;
	ctx.fillStyle = textColor;
	setCanvasLetterSpacing({ ctx, letterSpacingPx: layout.letterSpacing });

	// With keywords or rich text every line is laid out piece by piece,
	// since a larger piece widens its line; the background grows to the
	// widest one. While a typewriter animation reveals the text, keywords
	// and the spoken word are not shown (as on the desktop).
	const revealing = revealChars !== null;
	const activeKeywords = keywords && keywords.indices.length > 0 && !revealing ? keywords : null;
	const keyed =
		activeKeywords || (rich && rich.length > 0) ? styledLineRuns({ ctx, layout, keywords: activeKeywords, rich }) : null;
	const block = keyed
		? { ...layout.block, maxWidth: Math.max(layout.block.maxWidth, ...keyed.map((run) => run.width)) }
		: layout.block;

	if (
		background?.enabled &&
		backgroundColor &&
		backgroundColor !== "transparent" &&
		layout.lines.length > 0
	) {
		const backgroundRect = getTextBackgroundRect({
			textAlign: layout.textAlign,
			block,
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

	if (keyed) {
		drawStyledLines({
			ctx,
			layout,
			runs: keyed,
			keywords: activeKeywords,
			highlight: revealing ? null : (highlight ?? null),
			textColor,
			revealChars,
		});
		return;
	}

	let remaining = revealChars ?? Number.POSITIVE_INFINITY;
	for (let index = 0; index < layout.lines.length; index++) {
		const lineY = index * layout.lineHeightPx - layout.block.visualCenterOffset;
		const chars = Array.from(layout.lines[index]);
		if (remaining < chars.length) {
			// Part of the line: draw its start where it sits in the whole line.
			if (remaining > 0) {
				const lineWidth = layout.lineMetrics[index].width;
				const x = layout.textAlign === "center" ? -lineWidth / 2 : layout.textAlign === "right" ? -lineWidth : 0;
				ctx.save();
				ctx.textAlign = "left";
				ctx.fillText(chars.slice(0, remaining).join(""), x, lineY);
				ctx.restore();
			}
			remaining = 0;
			continue;
		}
		remaining -= chars.length;
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

	if (highlight && highlight.mode !== "none" && revealChars === null) {
		drawHighlightedWords({ ctx, layout, highlight, textColor });
	}
}

interface StyledPiece {
	text: string;
	/** Offset from the line's left edge. */
	x: number;
	width: number;
	font: string;
	/** Font size in pixels. */
	size: number;
	/** Its own rich-text colour, if any. */
	color: string | null;
	/** The caption word index, or null for the space between words. */
	wordIndex: number | null;
	keyword: boolean;
}

/** The base font with another weight, slant and size. */
function fontWith(base: string, { bold, italic, size }: { bold: boolean; italic: boolean; size: number }): string {
	return base.replace(/^(normal|italic) (normal|bold) (\d+(?:\.\d+)?)px/, `${italic ? "italic" : "normal"} ${bold ? "bold" : "normal"} ${size}px`);
}

function sameRich(a: RichCharStyle | undefined, b: RichCharStyle | undefined): boolean {
	return a?.color === b?.color && a?.bold === b?.bold && a?.italic === b?.italic && a?.scale === b?.scale;
}

/**
 * Each line as measured pieces: words split where their rich-text style
 * changes, each in its own font, and the spaces between words. Keywords
 * take their size on top of any rich-text size (`editor` draws the same).
 */
export function styledLineRuns({
	ctx,
	layout,
	keywords,
	rich,
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	keywords: TextKeywords | null;
	rich: RichCharStyle[] | null;
}): { width: number; pieces: StyledPiece[] }[] {
	const wanted = new Set(keywords?.indices ?? []);
	const baseFont = layout.fontString;
	const baseMatch = /^(normal|italic) (normal|bold) /.exec(baseFont);
	const baseItalic = baseMatch?.[1] === "italic";
	const baseBold = baseMatch?.[2] === "bold";
	const em = layout.scaledFontSize;
	let wordIndex = 0;
	let charIndex = 0;
	const runs = layout.lines.map((line, lineNumber) => {
		if (lineNumber > 0) charIndex += 1; // the newline
		const pieces: StyledPiece[] = [];
		let x = 0;
		for (const match of line.matchAll(/\S+|\s+/g)) {
			const isWord = /\S/.test(match[0]);
			const keyword = isWord && wanted.has(wordIndex);
			const chars = Array.from(match[0]);
			let from = 0;
			while (from < chars.length) {
				const own = rich?.[charIndex + from];
				let to = from + 1;
				while (to < chars.length && sameRich(rich?.[charIndex + to], own)) to += 1;
				const text = chars.slice(from, to).join("");
				const size = em * (own?.scale ?? 1) * (keyword && keywords ? keywords.scale : 1);
				const font = fontWith(baseFont, { bold: own?.bold ?? baseBold, italic: own?.italic ?? baseItalic, size });
				ctx.font = font;
				const width = ctx.measureText(text).width;
				pieces.push({ text, x, width, font, size, color: own?.color ?? null, wordIndex: isWord ? wordIndex : null, keyword });
				x += width;
				from = to;
			}
			charIndex += chars.length;
			if (isWord) wordIndex += 1;
		}
		return { width: x, pieces };
	});
	ctx.font = baseFont;
	return runs;
}

/**
 * Draws the lines piece by piece: rich-text styles, keywords in their
 * colour and size, the spoken word (and its box) on top; with a reveal,
 * only the characters shown so far.
 */
function drawStyledLines({
	ctx,
	layout,
	runs,
	keywords,
	highlight,
	textColor,
	revealChars,
}: {
	ctx: TextCanvasContext;
	layout: MeasuredTextLayout;
	runs: { width: number; pieces: StyledPiece[] }[];
	keywords: TextKeywords | null;
	highlight: TextWordHighlight | null;
	textColor: string;
	revealChars: number | null;
}): void {
	const spoken = new Set(highlight && highlight.mode !== "none" ? highlightedWordIndices({ highlight }) : []);
	let remaining = revealChars ?? Number.POSITIVE_INFINITY;
	// Pieces of a line share one baseline, set by the largest piece, as
	// cosmic-text lays out the desktop's text: a larger piece drops the
	// line's baseline by half its extra ascent less descent (`drop`), and
	// each piece is drawn (textBaseline middle) so its own baseline, `rise`
	// below its middle, lands there. Both scale with the size.
	const em = layout.scaledFontSize;
	const drawnBaseline = ctx.textBaseline;
	ctx.textBaseline = "alphabetic";
	const fromBaseline = ctx.measureText("Hg");
	ctx.textBaseline = "middle";
	const fromMiddle = ctx.measureText("Hg");
	ctx.textBaseline = drawnBaseline;
	const drop =
		em > 0 && fromBaseline.fontBoundingBoxAscent !== undefined && fromBaseline.fontBoundingBoxDescent !== undefined
			? (fromBaseline.fontBoundingBoxAscent - fromBaseline.fontBoundingBoxDescent) / 2 / em
			: 0.3465;
	const rise = em > 0 && Number.isFinite(fromMiddle.alphabeticBaseline) ? -fromMiddle.alphabeticBaseline / em : 0.2756;
	ctx.save();
	ctx.textAlign = "left";
	runs.forEach((run, line) => {
		const middleY = line * layout.lineHeightPx - layout.block.visualCenterOffset;
		const largest = Math.max(em, ...run.pieces.map((piece) => piece.size));
		// Where a piece of `size` is drawn (textBaseline middle) to sit on the line's baseline.
		const pieceY = (size: number) => middleY + rise * em + drop * (largest - em) - rise * size;
		const lineY = pieceY(em);
		const left = layout.textAlign === "center" ? -run.width / 2 : layout.textAlign === "right" ? -run.width : 0;
		if (highlight?.mode === "box" && highlight.activeIndex >= 0) {
			const active = run.pieces.filter((piece) => piece.wordIndex === highlight.activeIndex);
			if (active.length > 0) {
				const size = Math.max(...active.map((piece) => piece.size));
				const from = active[0].x;
				const to = active[active.length - 1].x + active[active.length - 1].width;
				const padX = size * HIGHLIGHT_BOX_PADDING_X;
				const height = size * HIGHLIGHT_BOX_HEIGHT + 2 * size * HIGHLIGHT_BOX_PADDING_Y;
				const boxY = pieceY(size);
				ctx.fillStyle = highlight.boxColor;
				ctx.beginPath();
				ctx.roundRect(left + from - padX, boxY - height / 2, to - from + 2 * padX, height, size * HIGHLIGHT_BOX_RADIUS);
				ctx.fill();
			}
		}
		for (const piece of run.pieces) {
			const chars = Array.from(piece.text);
			const shown = Math.min(chars.length, Math.max(0, remaining));
			remaining -= chars.length;
			if (piece.wordIndex === null && piece.color === null) continue;
			if (shown <= 0) continue;
			const isSpoken = piece.wordIndex !== null && spoken.has(piece.wordIndex);
			ctx.font = piece.font;
			ctx.fillStyle =
				isSpoken && highlight
					? highlight.activeColor
					: piece.keyword && keywords
						? keywords.color
						: (piece.color ?? textColor);
			ctx.fillText(shown < chars.length ? chars.slice(0, shown).join("") : piece.text, left + piece.x, pieceY(piece.size));
		}
		ctx.font = layout.fontString;
		ctx.textAlign = layout.textAlign;
		ctx.fillStyle = textColor;
		if (revealChars === null) {
			drawTextDecoration({
				ctx,
				textDecoration: layout.textDecoration,
				lineWidth: run.width,
				lineY,
				metrics: layout.lineMetrics[line],
				scaledFontSize: layout.scaledFontSize,
				textAlign: layout.textAlign,
			});
		}
		ctx.textAlign = "left";
	});
	ctx.restore();
	ctx.fillStyle = textColor;
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

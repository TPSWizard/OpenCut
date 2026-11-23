/**
 * Cover maker: a frame of the project with a title, as a PNG and as the
 * MP4's cover picture. This file holds the parts both editors share
 * (`crates/editor/src/cover.rs` is the same, and both are checked against
 * `fixtures/cover/cases.json`): the cover sizes, the title's layout for each
 * style, and the score that suggests a frame (sharp, colourful, well
 * exposed).
 */

export type CoverStyle = "bold" | "band" | "top";
export const COVER_STYLES: readonly CoverStyle[] = ["bold", "band", "top"];
export type CoverShape = "canvas" | "landscape" | "portrait";
export const COVER_SHAPES: readonly CoverShape[] = ["canvas", "landscape", "portrait"];

/** The cover a project keeps in its settings. */
export interface CoverSettings {
	/** Timeline seconds of the frame. */
	time: number;
	title: string;
	style: CoverStyle;
	/** Embed it as the cover picture of MP4 exports (at the canvas shape). */
	embed: boolean;
}

export const DEFAULT_COVER: CoverSettings = { time: 0, title: "", style: "bold", embed: true };

export interface Size {
	width: number;
	height: number;
}

/** The pixel size of a cover shape. */
export function coverSize(shape: CoverShape, canvas: Size): Size {
	if (shape === "landscape") return { width: 1280, height: 720 };
	if (shape === "portrait") return { width: 1080, height: 1920 };
	return { width: canvas.width, height: canvas.height };
}

const STYLE_FONT: Record<CoverStyle, number> = { bold: 0.11, band: 0.075, top: 0.09 };
const STYLE_COLOR: Record<CoverStyle, string> = { bold: "#ffffff", band: "#ffffff", top: "#ffd60a" };
const STYLE_OUTLINE: Record<CoverStyle, number> = { bold: 0.08, band: 0, top: 0.1 };
export const MAX_TITLE_LINES = 3;

/** Words wrapped greedily to `maxChars`; a longer word keeps its own line; past `maxLines` the last line ends with an ellipsis. */
export function wrapTitle(text: string, maxChars: number, maxLines = MAX_TITLE_LINES): string[] {
	const lines: string[] = [];
	for (const word of text.split(/\s+/).filter(Boolean)) {
		const last = lines[lines.length - 1];
		if (last !== undefined && last.length + 1 + word.length <= maxChars) lines[lines.length - 1] = `${last} ${word}`;
		else lines.push(word);
	}
	if (lines.length > maxLines) {
		const rest = lines.slice(maxLines - 1).join(" ");
		const cut = rest.slice(0, Math.max(1, maxChars - 1)).trimEnd();
		return [...lines.slice(0, maxLines - 1), `${cut}…`];
	}
	return lines;
}

export interface CoverTitle {
	lines: string[];
	maxChars: number;
	/** Font size in cover pixels. */
	fontPx: number;
	/** The text element's `fontSize` (a share of the height / 90). */
	fontSize: number;
	/** The text element's `transform.positionY` (from the centre). */
	positionY: number;
	color: string;
	outlineWidth: number;
	background?: { color: string; paddingX: number; paddingY: number; cornerRadius: number };
}

/** Where and how the title goes on a cover of `width` × `height`. */
export function coverTitle({ width, height, style, title }: Size & { style: CoverStyle; title: string }): CoverTitle {
	const fontPx = Math.min(width, height) * STYLE_FONT[style];
	const maxChars = Math.max(8, Math.floor((width * 0.86) / (fontPx * 0.56)));
	const lines = wrapTitle(title.split(/\s+/).filter(Boolean).join(" "), maxChars);
	const block = lines.length * fontPx * 1.2;
	const positionY =
		style === "bold"
			? height / 2 - 0.1 * height - block / 2
			: style === "band"
				? height / 2 - 0.08 * height - block / 2
				: -height / 2 + 0.08 * height + block / 2;
	return {
		lines,
		maxChars,
		fontPx,
		fontSize: (fontPx * 90) / height,
		positionY,
		color: STYLE_COLOR[style],
		outlineWidth: fontPx * STYLE_OUTLINE[style],
		...(style === "band" ? { background: { color: "#000000", paddingX: fontPx * 0.6, paddingY: fontPx * 0.3, cornerRadius: fontPx * 0.2 } } : {}),
	};
}

export interface CoverScore {
	sharpness: number;
	colourfulness: number;
	exposure: number;
	contrast: number;
	score: number;
}

/** How good an RGBA frame is as a cover: sharp, colourful, well exposed, with contrast. */
export function frameScore(rgba: ArrayLike<number>, width: number, height: number): CoverScore {
	const n = width * height;
	if (n === 0) return { sharpness: 0, colourfulness: 0, exposure: 0, contrast: 0, score: 0 };
	const lum = new Float64Array(n);
	let sumY = 0;
	let sumY2 = 0;
	let sumRg = 0;
	let sumRg2 = 0;
	let sumYb = 0;
	let sumYb2 = 0;
	for (let i = 0; i < n; i++) {
		const [r, g, b] = [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]];
		const y = 0.299 * r + 0.587 * g + 0.114 * b;
		lum[i] = y;
		sumY += y;
		sumY2 += y * y;
		const rg = r - g;
		const yb = 0.5 * (r + g) - b;
		sumRg += rg;
		sumRg2 += rg * rg;
		sumYb += yb;
		sumYb2 += yb * yb;
	}
	const meanY = sumY / n;
	const stdY = Math.sqrt(Math.max(0, sumY2 / n - meanY * meanY));
	let sharpness = 0;
	if (width >= 3 && height >= 3) {
		let sum = 0;
		let sum2 = 0;
		let count = 0;
		for (let y = 1; y < height - 1; y++) {
			for (let x = 1; x < width - 1; x++) {
				const i = y * width + x;
				const lap = 4 * lum[i] - lum[i - 1] - lum[i + 1] - lum[i - width] - lum[i + width];
				sum += lap;
				sum2 += lap * lap;
				count++;
			}
		}
		const mean = sum / count;
		sharpness = Math.min(1, Math.sqrt(Math.max(0, sum2 / count - mean * mean)) / 40);
	}
	const muRg = sumRg / n;
	const muYb = sumYb / n;
	const varRg = Math.max(0, sumRg2 / n - muRg * muRg);
	const varYb = Math.max(0, sumYb2 / n - muYb * muYb);
	const colourfulness = Math.min(1, (Math.sqrt(varRg + varYb) + 0.3 * Math.sqrt(muRg * muRg + muYb * muYb)) / 100);
	const exposure = Math.max(0, 1 - Math.abs(meanY - 128) / 128);
	const contrast = Math.min(1, stdY / 50);
	return { sharpness, colourfulness, exposure, contrast, score: exposure * (0.4 * sharpness + 0.35 * colourfulness + 0.25 * contrast) };
}

/** Frames looked at when suggesting one. */
export const COVER_SAMPLES = 16;

/** The best-scoring time, away from the first and last 5 % when there is one there. */
export function pickCoverTime(samples: readonly { time: number; score: number }[], duration: number): number {
	const inside = samples.filter((sample) => sample.time >= 0.05 * duration && sample.time <= 0.95 * duration);
	let best: { time: number; score: number } | null = null;
	for (const sample of inside.length ? inside : samples) {
		if (!best || sample.score > best.score) best = sample;
	}
	return best?.time ?? 0;
}

/** The sample times for a suggestion: evenly spaced, centred in their slots. */
export function coverSampleTimes(duration: number, count = COVER_SAMPLES): number[] {
	return Array.from({ length: count }, (_, index) => ((index + 0.5) / count) * duration);
}

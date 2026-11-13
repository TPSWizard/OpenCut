/**
 * Voice-over from a script: the script is cut into lines (sentences, long
 * ones broken at a comma or a space) that the voice reads one by one, with
 * a short pause after each sentence and a longer one at a paragraph break.
 * Knowing where each line starts in the joined audio gives captions timed
 * line by line. The desktop editor's `crates/editor/src/voiceover.rs` is
 * the same arithmetic; keep the two in step.
 */

export const MAX_LINE_CHARS = 180;
export const SENTENCE_PAUSE = 0.25;
export const PARAGRAPH_PAUSE = 0.6;

export interface ScriptLine {
	text: string;
	/** Silence after this line, seconds (0 after the last). */
	pauseAfter: number;
}

const ENDERS = new Set([".", "!", "?", "…", "。", "！", "？"]);
const FULL_WIDTH_ENDERS = new Set(["。", "！", "？"]);

function sentences(paragraph: string): string[] {
	const chars = Array.from(paragraph);
	const out: string[] = [];
	let current = "";
	chars.forEach((c, i) => {
		current += c;
		const next = chars[i + 1];
		const ends =
			ENDERS.has(c) &&
			(next === undefined || next === " " || FULL_WIDTH_ENDERS.has(c)) &&
			!(next !== undefined && ENDERS.has(next));
		if (ends) {
			const trimmed = current.trim();
			if (trimmed) out.push(trimmed);
			current = "";
		}
	});
	const trimmed = current.trim();
	if (trimmed) out.push(trimmed);
	return out;
}

function breakLong(line: string, max: number): string[] {
	const chars = Array.from(line);
	if (chars.length <= max) return [line];
	const middle = Math.floor(chars.length / 2);
	const nearest = (accept: (i: number) => boolean): number | null => {
		let best: number | null = null;
		for (let i = 1; i < chars.length - 1; i++) {
			if (!accept(i)) continue;
			if (best === null || Math.abs(i - middle) < Math.abs(best - middle)) best = i;
		}
		return best;
	};
	const comma = nearest((i) => [",", ";", "，", "；"].includes(chars[i]) && chars[i + 1] === " ");
	const cut = comma !== null ? comma + 1 : (nearest((i) => chars[i] === " ") ?? middle);
	return [
		...breakLong(chars.slice(0, cut).join("").trim(), max),
		...breakLong(chars.slice(cut).join("").trim(), max),
	];
}

export function splitScript({ script, maxChars = MAX_LINE_CHARS }: { script: string; maxChars?: number }): ScriptLine[] {
	const paragraphs = script
		.replace(/\r\n/g, "\n")
		.split("\n\n")
		.map((paragraph) => paragraph.split(/\s+/).filter(Boolean).join(" "))
		.filter(Boolean);
	const lines: ScriptLine[] = [];
	for (const paragraph of paragraphs) {
		const pieces = sentences(paragraph).flatMap((sentence) => breakLong(sentence, Math.max(20, maxChars)));
		pieces.forEach((text, index) => {
			lines.push({ text, pauseAfter: index + 1 === pieces.length ? PARAGRAPH_PAUSE : SENTENCE_PAUSE });
		});
	}
	if (lines.length > 0) lines[lines.length - 1].pauseAfter = 0;
	return lines;
}

/** Where each line plays in the joined audio, `[start, end]` seconds, and the total length. */
export function layoutLines({ lines, durations }: { lines: ScriptLine[]; durations: number[] }): {
	spans: Array<[number, number]>;
	total: number;
} {
	let cursor = 0;
	const spans: Array<[number, number]> = [];
	lines.forEach((line, index) => {
		const end = cursor + Math.max(0, durations[index] ?? 0);
		spans.push([cursor, end]);
		cursor = end + line.pauseAfter;
	});
	return { spans, total: cursor };
}

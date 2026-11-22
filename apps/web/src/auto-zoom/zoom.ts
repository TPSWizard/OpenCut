/**
 * Auto zoom punch-ins: small zooms in and out on the speaker at sentence
 * starts and on emphasis, as talking-head shorts do. A clip's punch-ins
 * are params, worked out as it plays (like the one-click animations), so
 * they sit on top of its keyframes and auto-reframe without touching them:
 *
 * - `zoom.punches`: "start-end@scale;…" in clip seconds, e.g.
 *   "1.20-4.50@1.150;6.00-8.20@1.300".
 * - `zoom.focusX` / `zoom.focusY`: the point zoomed into, as a share of the
 *   clip's frame from its centre (-0.5 to 0.5; the default sits a little
 *   above the middle, where a face usually is).
 * - `zoom.ramp`: seconds each zoom takes to go in and to come back out
 *   (0 cuts straight in), at most half the punch.
 *
 * The planner and the maths are the same as `crates/editor/src/auto_zoom.rs`
 * (both are checked against `fixtures/auto-zoom/cases.json`).
 */

export interface ZoomPunch {
	/** Clip seconds. */
	start: number;
	end: number;
	scale: number;
}

export interface ClipZoom {
	punches: ZoomPunch[];
	focusX: number;
	focusY: number;
	ramp: number;
}

export const ZOOM_PUNCHES_PARAM = "zoom.punches";
export const ZOOM_FOCUS_X_PARAM = "zoom.focusX";
export const ZOOM_FOCUS_Y_PARAM = "zoom.focusY";
export const ZOOM_RAMP_PARAM = "zoom.ramp";
export const ZOOM_FOCUS_Y_DEFAULT = -0.1;
export const ZOOM_RAMP_DEFAULT = 0.15;
export const ZOOM_RAMP_MAX = 1;
export const ZOOM_SCALE_MAX = 3;

export type ZoomStrength = "subtle" | "medium" | "strong";
export const ZOOM_STRENGTHS: readonly ZoomStrength[] = ["subtle", "medium", "strong"];
/** The zoom for an ordinary punch and for an emphasised one. */
export const ZOOM_LEVELS: Record<ZoomStrength, { normal: number; emphasis: number }> = {
	subtle: { normal: 1.08, emphasis: 1.15 },
	medium: { normal: 1.15, emphasis: 1.3 },
	strong: { normal: 1.25, emphasis: 1.5 },
};

/** A pause this long between words starts a new sentence. */
export const SENTENCE_GAP_SECONDS = 0.7;
/** Punches shorter than this are dropped. */
export const MIN_PUNCH_SECONDS = 0.8;
/** How long the last punch holds after the last word. */
export const TAIL_SECONDS = 0.5;

export interface ZoomWord {
	text: string;
	/** Clip seconds. */
	start: number;
	end: number;
	keyword?: boolean;
}

const round = (value: number, places: number) => Math.round(value * 10 ** places) / 10 ** places;

export function formatPunches(punches: readonly ZoomPunch[]): string {
	return punches.map((punch) => `${round(punch.start, 2).toFixed(2)}-${round(punch.end, 2).toFixed(2)}@${round(punch.scale, 3).toFixed(3)}`).join(";");
}

/** The punches of a `zoom.punches` string; malformed or empty entries are skipped. */
export function parsePunches(text: unknown): ZoomPunch[] {
	if (typeof text !== "string") return [];
	const punches: ZoomPunch[] = [];
	for (const part of text.split(";")) {
		const match = /^\s*(-?[\d.]+)\s*-\s*(-?[\d.]+)\s*@\s*([\d.]+)\s*$/.exec(part);
		if (!match) continue;
		const [start, end, scale] = [Number(match[1]), Number(match[2]), Number(match[3])];
		if (![start, end, scale].every(Number.isFinite) || end <= start || scale <= 0) continue;
		punches.push({ start, end, scale: Math.min(ZOOM_SCALE_MAX, scale) });
	}
	return punches.sort((a, b) => a.start - b.start);
}

const finite = (value: unknown, fallback: number) => (typeof value === "number" && Number.isFinite(value) ? value : fallback);
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** The clip's punch-ins, or null when it has none. */
export function readClipZoom(params: Record<string, unknown>): ClipZoom | null {
	const punches = parsePunches(params[ZOOM_PUNCHES_PARAM]);
	if (punches.length === 0) return null;
	return {
		punches,
		focusX: clamp(finite(params[ZOOM_FOCUS_X_PARAM], 0), -0.5, 0.5),
		focusY: clamp(finite(params[ZOOM_FOCUS_Y_PARAM], ZOOM_FOCUS_Y_DEFAULT), -0.5, 0.5),
		ramp: clamp(finite(params[ZOOM_RAMP_PARAM], ZOOM_RAMP_DEFAULT), 0, ZOOM_RAMP_MAX),
	};
}

const smoothstep = (t: number) => {
	const x = clamp(t, 0, 1);
	return x * x * (3 - 2 * x);
};

/** The zoom (1 = none) at `seconds` into the clip. */
export function zoomAt({ zoom, seconds }: { zoom: ClipZoom; seconds: number }): number {
	for (const punch of zoom.punches) {
		if (seconds < punch.start || seconds >= punch.end) continue;
		const ramp = Math.min(zoom.ramp, (punch.end - punch.start) / 2);
		const amount = ramp <= 0 ? 1 : Math.min(smoothstep((seconds - punch.start) / ramp), smoothstep((punch.end - seconds) / ramp));
		return 1 + (punch.scale - 1) * amount;
	}
	return 1;
}

/**
 * How the zoom changes a frame of `width` × `height` (its drawn size, before
 * the zoom) turned by `rotate` degrees: the scale factor and the offset that
 * keeps the focus point where it was.
 */
export function zoomTransform({
	zoom,
	scale,
	width,
	height,
	rotate,
}: {
	zoom: ClipZoom;
	scale: number;
	width: number;
	height: number;
	rotate: number;
}): { scale: number; offsetX: number; offsetY: number } {
	const localX = (1 - scale) * zoom.focusX * width;
	const localY = (1 - scale) * zoom.focusY * height;
	const angle = (rotate * Math.PI) / 180;
	return {
		scale,
		offsetX: localX * Math.cos(angle) - localY * Math.sin(angle),
		offsetY: localX * Math.sin(angle) + localY * Math.cos(angle),
	};
}

const endsSentence = (text: string) => /[.!?]["'”’)]*$/u.test(text.trim());

/**
 * Punch-ins from the clip's spoken words: a sentence starts after . ! ? or
 * a pause of `SENTENCE_GAP_SECONDS`; sentences alternate between no zoom and
 * the normal zoom, and one with a keyword, a number or an exclamation mark
 * takes the emphasis zoom. Each holds until the next sentence starts (the
 * last for `TAIL_SECONDS` after its last word), punches shorter than
 * `MIN_PUNCH_SECONDS` are dropped and touching punches of one zoom merge.
 */
export function planPunches({
	words,
	duration,
	strength = "medium",
}: {
	words: readonly ZoomWord[];
	duration: number;
	strength?: ZoomStrength;
}): ZoomPunch[] {
	const levels = ZOOM_LEVELS[strength];
	const sorted = words.filter((word) => word.text.trim() && word.end > 0 && word.start < duration).sort((a, b) => a.start - b.start);
	const sentences: ZoomWord[][] = [];
	for (const word of sorted) {
		const current = sentences[sentences.length - 1];
		const previous = current?.[current.length - 1];
		if (!current || !previous || endsSentence(previous.text) || word.start - previous.end >= SENTENCE_GAP_SECONDS) {
			sentences.push([word]);
		} else {
			current.push(word);
		}
	}
	const punches: ZoomPunch[] = [];
	sentences.forEach((sentence, index) => {
		const emphasis = sentence.some((word) => word.keyword || /\p{N}/u.test(word.text) || word.text.includes("!"));
		const scale = emphasis ? levels.emphasis : index % 2 === 1 ? levels.normal : 1;
		if (scale === 1) return;
		const start = Math.max(0, sentence[0].start);
		const next = sentences[index + 1];
		const end = Math.min(duration, next ? next[0].start : sentence[sentence.length - 1].end + TAIL_SECONDS);
		const last = punches[punches.length - 1];
		if (last && last.scale === scale && Math.abs(last.end - start) < 1e-6) {
			last.end = end;
			return;
		}
		punches.push({ start, end, scale });
	});
	return punches.filter((punch) => punch.end - punch.start >= MIN_PUNCH_SECONDS);
}

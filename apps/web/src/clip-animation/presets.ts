import type { ParamValues } from "@/params";
import type { Transform } from "@/rendering";

/**
 * One-click animations for a clip: a way in over its first moments, a way
 * out over its last, and a loop over the whole clip. They are worked out
 * while the clip plays, on top of its keyframes, from six element params:
 * `animIn` / `animInDuration`, `animOut` / `animOutDuration` and
 * `animLoop` / `animLoopPeriod`. So they follow the clip when it is
 * trimmed or moved, never clash with keyframes the user set, and come off
 * by setting the kind back to "none". The desktop editor's
 * `editor::clip_animation` is the same maths.
 */

export const ANIMATION_IN_KINDS = [
	"fade",
	"pop",
	"grow",
	"shrink",
	"slideUp",
	"slideDown",
	"slideLeft",
	"slideRight",
	"bounce",
	"spin",
	"typewriter",
	"wordByWord",
] as const;
export const ANIMATION_OUT_KINDS = [
	"fade",
	"pop",
	"grow",
	"shrink",
	"slideUp",
	"slideDown",
	"slideLeft",
	"slideRight",
	"spin",
] as const;
export const ANIMATION_LOOP_KINDS = ["pulse", "shake", "wobble", "float", "kenBurns"] as const;

export type AnimationInKind = (typeof ANIMATION_IN_KINDS)[number];
export type AnimationOutKind = (typeof ANIMATION_OUT_KINDS)[number];
export type AnimationLoopKind = (typeof ANIMATION_LOOP_KINDS)[number];
export type AnimationSlot = "in" | "out" | "loop";

/** Presets that only make sense on text (they reveal it letter by letter or word by word). */
export const TEXT_ONLY_ANIMATIONS: ReadonlySet<string> = new Set(["typewriter", "wordByWord"]);

export const ANIMATION_DURATION_DEFAULT = 0.5;
export const ANIMATION_DURATION_MIN = 0.1;
export const ANIMATION_DURATION_MAX = 5;
export const ANIMATION_LOOP_PERIOD_DEFAULT = 1;
export const ANIMATION_LOOP_PERIOD_MIN = 0.2;
export const ANIMATION_LOOP_PERIOD_MAX = 10;

export const ANIMATION_LABELS: Record<string, string> = {
	fade: "Fade",
	pop: "Pop",
	grow: "Grow",
	shrink: "Shrink",
	slideUp: "Slide up",
	slideDown: "Slide down",
	slideLeft: "Slide left",
	slideRight: "Slide right",
	bounce: "Drop and bounce",
	spin: "Spin",
	typewriter: "Typewriter",
	wordByWord: "Word by word",
	pulse: "Pulse",
	shake: "Shake",
	wobble: "Wobble",
	float: "Float",
	kenBurns: "Ken Burns",
};

export interface ClipAnimations {
	in: { kind: AnimationInKind; seconds: number } | null;
	out: { kind: AnimationOutKind; seconds: number } | null;
	loop: { kind: AnimationLoopKind; period: number } | null;
}

/** How a clip looks at one moment, relative to its keyframed state. */
export interface ClipAnimationState {
	/** Multiplies the opacity. */
	opacity: number;
	/** Multiplies both scales. */
	scale: number;
	/** Canvas pixels added to the position. */
	offsetX: number;
	offsetY: number;
	/** Degrees added to the rotation. */
	rotate: number;
	/** How much of the text shows, or null when all of it does. */
	reveal: { mode: "chars" | "words"; fraction: number } | null;
}

function readNumber(value: unknown, fallback: number, min: number, max: number): number {
	const number = typeof value === "number" && Number.isFinite(value) ? value : fallback;
	return Math.min(max, Math.max(min, number));
}

function readKind<T extends string>(value: unknown, kinds: readonly T[]): T | null {
	return typeof value === "string" && (kinds as readonly string[]).includes(value) ? (value as T) : null;
}

/** The clip's animations from its params, or null when it has none. */
export function readClipAnimations(params: ParamValues | undefined): ClipAnimations | null {
	if (!params) return null;
	const inKind = readKind(params.animIn, ANIMATION_IN_KINDS);
	const outKind = readKind(params.animOut, ANIMATION_OUT_KINDS);
	const loopKind = readKind(params.animLoop, ANIMATION_LOOP_KINDS);
	if (!inKind && !outKind && !loopKind) return null;
	const duration = (value: unknown) =>
		readNumber(value, ANIMATION_DURATION_DEFAULT, ANIMATION_DURATION_MIN, ANIMATION_DURATION_MAX);
	return {
		in: inKind ? { kind: inKind, seconds: duration(params.animInDuration) } : null,
		out: outKind ? { kind: outKind, seconds: duration(params.animOutDuration) } : null,
		loop: loopKind
			? {
					kind: loopKind,
					period: readNumber(
						params.animLoopPeriod,
						ANIMATION_LOOP_PERIOD_DEFAULT,
						ANIMATION_LOOP_PERIOD_MIN,
						ANIMATION_LOOP_PERIOD_MAX,
					),
				}
			: null,
	};
}

const easeOutCubic = (p: number) => 1 - (1 - p) ** 3;
const C1 = 1.70158;
const C3 = C1 + 1;
const easeOutBack = (p: number) => 1 + C3 * (p - 1) ** 3 + C1 * (p - 1) ** 2;
const easeInOutSine = (p: number) => -(Math.cos(Math.PI * p) - 1) / 2;
function easeOutBounce(p: number): number {
	const n1 = 7.5625;
	const d1 = 2.75;
	if (p < 1 / d1) return n1 * p * p;
	if (p < 2 / d1) {
		const q = p - 1.5 / d1;
		return n1 * q * q + 0.75;
	}
	if (p < 2.5 / d1) {
		const q = p - 2.25 / d1;
		return n1 * q * q + 0.9375;
	}
	const q = p - 2.625 / d1;
	return n1 * q * q + 0.984375;
}
const lerp = (from: number, to: number, t: number) => from + (to - from) * t;
const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

/** Share of the canvas a slide travels, and of its height a drop falls from. */
const SLIDE_DISTANCE = 0.3;

/**
 * Applies an in or out preset at `progress` (0 = gone, 1 = fully in place;
 * an out preset's progress runs from 1 down to 0 over its last moments).
 */
function applyEdge({
	state,
	kind,
	slot,
	progress,
	canvasWidth,
	canvasHeight,
	isText,
}: {
	state: ClipAnimationState;
	kind: string;
	slot: "in" | "out";
	progress: number;
	canvasWidth: number;
	canvasHeight: number;
	isText: boolean;
}): void {
	const v = clamp01(progress);
	const eased = easeOutCubic(v);
	// In: the clip arrives from the slide's far side; out: it leaves towards it.
	const side = slot === "in" ? 1 : -1;
	const slideX = SLIDE_DISTANCE * canvasWidth * (1 - eased);
	const slideY = SLIDE_DISTANCE * canvasHeight * (1 - eased);
	switch (kind) {
		case "fade":
			state.opacity *= v;
			return;
		case "pop":
			state.scale *= Math.max(0, easeOutBack(v));
			state.opacity *= Math.min(1, 4 * v);
			return;
		case "grow":
			state.scale *= lerp(slot === "in" ? 0.5 : 1.6, 1, eased);
			state.opacity *= v;
			return;
		case "shrink":
			state.scale *= lerp(slot === "in" ? 1.6 : 0.5, 1, eased);
			state.opacity *= v;
			return;
		case "slideUp":
			state.offsetY += side * slideY;
			state.opacity *= Math.min(1, 2 * v);
			return;
		case "slideDown":
			state.offsetY -= side * slideY;
			state.opacity *= Math.min(1, 2 * v);
			return;
		case "slideLeft":
			state.offsetX += side * slideX;
			state.opacity *= Math.min(1, 2 * v);
			return;
		case "slideRight":
			state.offsetX -= side * slideX;
			state.opacity *= Math.min(1, 2 * v);
			return;
		case "bounce":
			state.offsetY -= SLIDE_DISTANCE * canvasHeight * (1 - easeOutBounce(v));
			state.opacity *= Math.min(1, 4 * v);
			return;
		case "spin":
			state.rotate -= side * 180 * (1 - eased);
			state.scale *= lerp(0.3, 1, eased);
			state.opacity *= v;
			return;
		case "typewriter":
		case "wordByWord":
			if (isText) {
				state.reveal = { mode: kind === "typewriter" ? "chars" : "words", fraction: v };
			} else {
				state.opacity *= v;
			}
			return;
	}
}

function applyLoop({
	state,
	kind,
	period,
	localSeconds,
	lengthSeconds,
	canvasWidth,
	canvasHeight,
}: {
	state: ClipAnimationState;
	kind: AnimationLoopKind;
	period: number;
	localSeconds: number;
	lengthSeconds: number;
	canvasWidth: number;
	canvasHeight: number;
}): void {
	const angle = (2 * Math.PI * localSeconds) / period;
	switch (kind) {
		case "pulse":
			state.scale *= 1 + 0.04 * (1 - Math.cos(angle));
			return;
		case "shake":
			state.offsetX += 0.01 * canvasWidth * (0.6 * Math.sin(3 * angle) + 0.4 * Math.sin(7 * angle));
			state.offsetY += 0.01 * canvasHeight * (0.6 * Math.sin(4 * angle) + 0.4 * Math.sin(9 * angle));
			return;
		case "wobble":
			state.rotate += 6 * Math.sin(angle);
			return;
		case "float":
			state.offsetY -= 0.02 * canvasHeight * Math.sin(angle);
			return;
		case "kenBurns": {
			// A slow push in with a drift over the whole clip; it stays
			// inside the edges of a full-frame picture.
			const eased = easeInOutSine(lengthSeconds > 0 ? clamp01(localSeconds / lengthSeconds) : 0);
			state.scale *= 1 + 0.15 * eased;
			state.offsetX -= 0.04 * canvasWidth * eased;
			state.offsetY -= 0.02 * canvasHeight * eased;
			return;
		}
	}
}

/**
 * The in and out windows in seconds, scaled down together when they would
 * overlap on a short clip.
 */
export function animationWindows({
	animations,
	lengthSeconds,
}: {
	animations: ClipAnimations;
	lengthSeconds: number;
}): { inSeconds: number; outSeconds: number } {
	let inSeconds = animations.in?.seconds ?? 0;
	let outSeconds = animations.out?.seconds ?? 0;
	const length = Math.max(0, lengthSeconds);
	if (inSeconds + outSeconds > length && inSeconds + outSeconds > 0) {
		const scale = length / (inSeconds + outSeconds);
		inSeconds *= scale;
		outSeconds *= scale;
	}
	return { inSeconds, outSeconds };
}

/** The clip's animated state `localSeconds` into a clip `lengthSeconds` long. */
export function clipAnimationStateAt({
	animations,
	localSeconds,
	lengthSeconds,
	canvasWidth,
	canvasHeight,
	isText = false,
}: {
	animations: ClipAnimations;
	localSeconds: number;
	lengthSeconds: number;
	canvasWidth: number;
	canvasHeight: number;
	isText?: boolean;
}): ClipAnimationState {
	const state: ClipAnimationState = { opacity: 1, scale: 1, offsetX: 0, offsetY: 0, rotate: 0, reveal: null };
	const local = Math.min(Math.max(0, localSeconds), Math.max(0, lengthSeconds));
	const { inSeconds, outSeconds } = animationWindows({ animations, lengthSeconds });
	if (animations.loop) {
		applyLoop({
			state,
			kind: animations.loop.kind,
			period: animations.loop.period,
			localSeconds: local,
			lengthSeconds,
			canvasWidth,
			canvasHeight,
		});
	}
	if (animations.in && inSeconds > 0 && local < inSeconds) {
		applyEdge({
			state,
			kind: animations.in.kind,
			slot: "in",
			progress: local / inSeconds,
			canvasWidth,
			canvasHeight,
			isText,
		});
	}
	if (animations.out && outSeconds > 0 && local > lengthSeconds - outSeconds) {
		applyEdge({
			state,
			kind: animations.out.kind,
			slot: "out",
			progress: (lengthSeconds - local) / outSeconds,
			canvasWidth,
			canvasHeight,
			isText,
		});
	}
	return state;
}

/** `transform` and `opacity` with the animated state applied. */
export function applyClipAnimationState({
	transform,
	opacity,
	state,
}: {
	transform: Transform;
	opacity: number;
	state: ClipAnimationState;
}): { transform: Transform; opacity: number } {
	return {
		transform: {
			position: { x: transform.position.x + state.offsetX, y: transform.position.y + state.offsetY },
			scaleX: transform.scaleX * state.scale,
			scaleY: transform.scaleY * state.scale,
			rotate: transform.rotate + state.rotate,
		},
		opacity: Math.min(1, Math.max(0, opacity * state.opacity)),
	};
}

/**
 * How many characters of `content` show (newlines not counted), or null
 * when all of them do. Words are runs of non-space characters; a word
 * shows whole, with the spaces before it.
 */
export function revealedCharCount({
	content,
	reveal,
}: {
	content: string;
	reveal: ClipAnimationState["reveal"];
}): number | null {
	if (!reveal || reveal.fraction >= 1) return null;
	const chars = Array.from(content);
	if (reveal.mode === "chars") {
		const count = chars.filter((char) => char !== "\n").length;
		return Math.ceil(clamp01(reveal.fraction) * count);
	}
	// Counted without newlines, which still end a word.
	const wordEnds: number[] = [];
	let counted = 0;
	for (let index = 0; index < chars.length; index++) {
		if (chars[index] === "\n") continue;
		counted += 1;
		const isSpace = /\s/.test(chars[index]);
		const nextIsSpace = index + 1 >= chars.length || /\s/.test(chars[index + 1]);
		if (!isSpace && nextIsSpace) wordEnds.push(counted);
	}
	const shown = Math.ceil(clamp01(reveal.fraction) * wordEnds.length);
	return shown === 0 ? 0 : wordEnds[shown - 1];
}

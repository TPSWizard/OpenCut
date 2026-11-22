import { describe, expect, it } from "vitest";
import {
	animationWindows,
	applyClipAnimationState,
	clipAnimationStateAt,
	readClipAnimations,
	revealedCharCount,
} from "../presets";

const canvas = { canvasWidth: 1920, canvasHeight: 1080 };

describe("clip animation presets", () => {
	it("reads the params, ignoring none and unknown kinds", () => {
		expect(readClipAnimations({ animIn: "none", animOut: "none", animLoop: "none" })).toBeNull();
		expect(readClipAnimations({ animIn: "wiggle" })).toBeNull();
		const read = readClipAnimations({ animIn: "fade", animInDuration: 99, animLoop: "pulse", animLoopPeriod: 0 });
		expect(read).toEqual({
			in: { kind: "fade", seconds: 5 },
			out: null,
			loop: { kind: "pulse", period: 0.2 },
		});
	});

	it("fades in over its length and leaves the rest of the clip alone", () => {
		const animations = readClipAnimations({ animIn: "fade", animInDuration: 1 })!;
		const at = (localSeconds: number) =>
			clipAnimationStateAt({ animations, localSeconds, lengthSeconds: 4, ...canvas });
		expect(at(0).opacity).toBe(0);
		expect(at(0.5).opacity).toBeCloseTo(0.5, 9);
		expect(at(1).opacity).toBe(1);
		expect(at(3)).toEqual({ opacity: 1, scale: 1, offsetX: 0, offsetY: 0, rotate: 0, reveal: null });
	});

	it("slides in from below and leaves upwards", () => {
		const animations = readClipAnimations({ animIn: "slideUp", animOut: "slideUp" })!;
		const start = clipAnimationStateAt({ animations, localSeconds: 0, lengthSeconds: 4, ...canvas });
		expect(start.offsetY).toBeCloseTo(0.3 * 1080, 9);
		const end = clipAnimationStateAt({ animations, localSeconds: 4, lengthSeconds: 4, ...canvas });
		expect(end.offsetY).toBeCloseTo(-0.3 * 1080, 9);
		const middle = clipAnimationStateAt({ animations, localSeconds: 2, lengthSeconds: 4, ...canvas });
		expect(middle.offsetY).toBe(0);
	});

	it("pops in past full size before settling", () => {
		const animations = readClipAnimations({ animIn: "pop", animInDuration: 1 })!;
		const peak = Math.max(
			...[0.5, 0.6, 0.7, 0.8].map(
				(localSeconds) => clipAnimationStateAt({ animations, localSeconds, lengthSeconds: 3, ...canvas }).scale,
			),
		);
		expect(peak).toBeGreaterThan(1.05);
		expect(clipAnimationStateAt({ animations, localSeconds: 1, lengthSeconds: 3, ...canvas }).scale).toBeCloseTo(1, 9);
	});

	it("scales the in and out down together on a short clip", () => {
		const animations = readClipAnimations({ animIn: "fade", animInDuration: 2, animOut: "fade", animOutDuration: 2 })!;
		expect(animationWindows({ animations, lengthSeconds: 1 })).toEqual({ inSeconds: 0.5, outSeconds: 0.5 });
	});

	it("loops and pushes in with Ken Burns", () => {
		const pulse = readClipAnimations({ animLoop: "pulse", animLoopPeriod: 1 })!;
		expect(clipAnimationStateAt({ animations: pulse, localSeconds: 0, lengthSeconds: 5, ...canvas }).scale).toBe(1);
		expect(clipAnimationStateAt({ animations: pulse, localSeconds: 0.5, lengthSeconds: 5, ...canvas }).scale).toBeCloseTo(1.08, 9);
		const kenBurns = readClipAnimations({ animLoop: "kenBurns" })!;
		const end = clipAnimationStateAt({ animations: kenBurns, localSeconds: 5, lengthSeconds: 5, ...canvas });
		expect(end.scale).toBeCloseTo(1.15, 9);
		expect(end.offsetX).toBeCloseTo(-0.04 * 1920, 9);
	});

	it("applies the state on top of the keyframed transform", () => {
		const applied = applyClipAnimationState({
			transform: { position: { x: 10, y: 20 }, scaleX: -2, scaleY: 1, rotate: 5 },
			opacity: 0.8,
			state: { opacity: 0.5, scale: 1.5, offsetX: 1, offsetY: -2, rotate: 10, reveal: null },
		});
		expect(applied).toEqual({
			transform: { position: { x: 11, y: 18 }, scaleX: -3, scaleY: 1.5, rotate: 15 },
			opacity: 0.4,
		});
	});

	it("reveals text letter by letter or word by word", () => {
		const typewriter = readClipAnimations({ animIn: "typewriter", animInDuration: 1 })!;
		const state = clipAnimationStateAt({ animations: typewriter, localSeconds: 0.5, lengthSeconds: 3, ...canvas, isText: true });
		expect(state.reveal).toEqual({ mode: "chars", fraction: 0.5 });
		expect(state.opacity).toBe(1);
		// Not text: a typewriter falls back to a fade.
		expect(clipAnimationStateAt({ animations: typewriter, localSeconds: 0.5, lengthSeconds: 3, ...canvas }).opacity).toBe(0.5);

		expect(revealedCharCount({ content: "ab\ncd", reveal: { mode: "chars", fraction: 0.5 } })).toBe(2);
		expect(revealedCharCount({ content: "ab\ncd", reveal: { mode: "chars", fraction: 1 } })).toBeNull();
		const words = (fraction: number) => revealedCharCount({ content: "one two\nthree", reveal: { mode: "words", fraction } });
		expect(words(0)).toBe(0);
		expect(words(0.2)).toBe(3);
		expect(words(0.5)).toBe(7);
		expect(words(0.9)).toBe(12);
	});
});

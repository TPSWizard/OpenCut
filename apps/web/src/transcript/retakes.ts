/**
 * Retakes in a clip's transcript: earlier takes of a line and false starts
 * inside one, found by the shared `retakes` crate (through WASM) exactly as
 * on the desktop. The words found are struck in the transcript for review
 * and cut like any struck words (`./edit`), so only the last take stays.
 */

import { findRetakes } from "opencut-wasm";
import type { TranscriptWord } from "./cuts";

export interface RetakeOptions {
	/** A pause at least this long ends a phrase (seconds, default 0.7). */
	pauseSeconds?: number;
	/** Share of an earlier take's words that must come back (default 0.6). */
	similarity?: number;
	/** The later take starts at most this long after (seconds, default 30). */
	windowSeconds?: number;
	/** Look for false starts inside phrases too (default true). */
	restarts?: boolean;
}

export interface Retake {
	kind: "retake" | "restart";
	/** Indices into the transcript's words, in time order. */
	words: number[];
	/** Index of the first word of the take kept. */
	kept: number;
	similarity: number;
	start: number;
	end: number;
	text: string;
	keptText: string;
}

/** The retakes among the words, in time order, never overlapping. */
export function transcriptRetakes(words: TranscriptWord[], options: RetakeOptions = {}): Retake[] {
	const plain = words.map((word) => ({ text: word.text, start: word.start, end: word.end }));
	return JSON.parse(findRetakes(JSON.stringify(plain), JSON.stringify(options))) as Retake[];
}

/** Every word index the retakes cut, ascending. */
export function retakeWords(retakes: Retake[]): number[] {
	return [...new Set(retakes.flatMap((retake) => retake.words))].sort((a, b) => a - b);
}

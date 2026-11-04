import type { MediaTime } from "@/wasm";

/**
 * One spoken word inside a caption (text) element. Times are relative to
 * the element's start, so moving the clip keeps the words in step. The words
 * are the whitespace-separated tokens of the element's content, in order;
 * punctuation stays attached to its word.
 */
export interface CaptionWord {
	text: string;
	start: MediaTime;
	end: MediaTime;
}

/**
 * How the words of a caption are emphasised as they are spoken.
 * - `none`: plain text.
 * - `word`: the word being spoken takes the active colour.
 * - `box`: the word being spoken sits on a rounded box and takes the active colour.
 * - `fill`: every word spoken so far takes the active colour (karaoke).
 */
export type CaptionHighlight = "none" | "word" | "box" | "fill";

export const CAPTION_HIGHLIGHTS: readonly CaptionHighlight[] = [
	"none",
	"word",
	"box",
	"fill",
];

export function isCaptionHighlight(value: unknown): value is CaptionHighlight {
	return (
		typeof value === "string" &&
		(CAPTION_HIGHLIGHTS as readonly string[]).includes(value)
	);
}

/** The caption look stored in a text element's params (`caption.*`). */
export interface CaptionStyle {
	highlight: CaptionHighlight;
	activeColor: string;
	boxColor: string;
}

/** Where the playhead stands among a caption's words. */
export interface ActiveWordState {
	/** Index of the word being spoken, or -1 between words. */
	activeIndex: number;
	/** How many words have started (the active one included). */
	spokenCount: number;
}

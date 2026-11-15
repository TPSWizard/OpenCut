/**
 * Script to B-roll: a script becomes a shot list, one shot per line (the
 * voice-over's lines, see `@/voiceover/script`), each timed for reading at
 * a steady pace and given a stock search query. The desktop editor's
 * `crates/editor/src/broll.rs` is the same arithmetic, checked against
 * `fixtures/broll/cases.json`; keep the two in step.
 *
 * The query here is picked without a model: the line's words minus common
 * words, the longest three kept in their order. The API worker can ask
 * Claude for better queries for the same lines (`POST /broll/plan`); the
 * timings never change.
 */

import { MAX_LINE_CHARS, splitScript } from "@/voiceover/script";

/** Reading pace for timing shots, words a second (about 150 a minute). */
export const WORDS_PER_SECOND = 2.5;
/** No shot is shorter than this, seconds. */
export const MIN_SHOT_SECONDS = 1.5;
export const MAX_QUERY_CHARS = 60;
export const QUERY_WORDS = 3;

export interface Shot {
	/** The script line the shot covers. */
	text: string;
	/** What to search stock media for. */
	query: string;
	/** Seconds from the start of the B-roll. */
	start: number;
	duration: number;
}

const STOP_WORDS = new Set(
	`a about above after again all also am an and any are around as at back be because been before being below
between both but by can could did do does doing done down during each even every few first for from get gets
getting go goes going got had has have having he her here hers him his how i if in into is it its just know let
like made make makes many me might more most much must my need never new next no not now of off on once one only
or other our out over own really right same see she should so some still such take than that the their them then
there these they thing things think this those through to today too two under until up us use very want was way
we well were what when where which while who why will with would you your yours`.split(/\s+/),
);

const length = (text: string): number => [...text].length;

/** Words of a line: lower case, letters, digits and inner apostrophes or hyphens, possessive 's dropped. */
function words(text: string): string[] {
	return text
		.split(/[^\p{L}\p{N}'’-]+/u)
		.map((word) => word.replace(/^['’-]+|['’-]+$/g, "").toLowerCase())
		.map((word) => (word.endsWith("'s") || word.endsWith("’s") ? word.slice(0, -2) : word))
		.filter((word) => word.length > 0);
}

/**
 * The search query for a line: its longest uncommon words (three, at
 * least three letters, first appearance kept), in the order they appear.
 * A line of only common words, or in a script without spaces, is used as
 * it is, shortened.
 */
export function queryFor({ text }: { text: string }): string {
	const seen: string[] = [];
	for (const word of words(text)) {
		if (length(word) >= 3 && !STOP_WORDS.has(word) && !seen.includes(word)) seen.push(word);
	}
	const keep = seen
		.map((word, index) => ({ word, index }))
		.sort((a, b) => length(b.word) - length(a.word) || a.index - b.index)
		.slice(0, QUERY_WORDS)
		.sort((a, b) => a.index - b.index);
	const query = keep.length === 0 ? text.trim() : keep.map(({ word }) => word).join(" ");
	return [...query].slice(0, MAX_QUERY_CHARS).join("").trim();
}

const roundMs = (value: number): number => Math.round(value * 1000) / 1000;

/** How long a line plays at `wordsPerSecond`. */
export function shotSeconds({ text, wordsPerSecond = WORDS_PER_SECOND }: { text: string; wordsPerSecond?: number }): number {
	const pace = Number.isFinite(wordsPerSecond) && wordsPerSecond > 0 ? wordsPerSecond : WORDS_PER_SECOND;
	const spaced = text.split(/\s+/).filter(Boolean).length;
	// Scripts without spaces (Chinese, Japanese): about three characters a word.
	const count = spaced <= 1 ? Math.max(1, length(text) / 3) : spaced;
	return roundMs(Math.max(count / pace, MIN_SHOT_SECONDS));
}

/** The shot list for `script`: one shot per line, back to back with the voice-over's pauses between them. */
export function planShots({ script, wordsPerSecond = WORDS_PER_SECOND }: { script: string; wordsPerSecond?: number }): Shot[] {
	let start = 0;
	return splitScript({ script, maxChars: MAX_LINE_CHARS }).map((line) => {
		const duration = shotSeconds({ text: line.text, wordsPerSecond });
		const shot = { text: line.text, query: queryFor({ text: line.text }), start: roundMs(start), duration };
		start += duration + line.pauseAfter;
		return shot;
	});
}

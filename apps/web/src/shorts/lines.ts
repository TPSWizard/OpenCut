/**
 * Long video to shorts, step one: the transcript as timed lines (whole
 * sentences, or pieces of long ones), and picking the moments to cut.
 * Claude picks them through the API worker (`POST /shorts/plan`); without
 * the server passcode a simple local pick is used instead.
 */

import type { TranscriptWord } from "@/transcript/cuts";

export interface TimedLine {
	text: string;
	/** Timeline seconds. */
	start: number;
	end: number;
}

export interface ShortPick {
	/** First and last line of the short. */
	first: number;
	last: number;
	start: number;
	end: number;
	title: string;
	reason: string;
}

export interface ShortsOptions {
	count: number;
	minSeconds: number;
	maxSeconds: number;
}

export const SHORTS_DEFAULTS: ShortsOptions = { count: 3, minSeconds: 15, maxSeconds: 60 };

const SENTENCE_END = /[.!?。！？…]["')\]]*$/;

/**
 * Words to lines: a line ends at the end of a sentence, at a pause of
 * `pauseSeconds` or more, or once it reaches `maxSeconds` (so the picks can
 * be fine-grained even without punctuation).
 */
export function transcriptLines({
	words,
	pauseSeconds = 0.8,
	maxSeconds = 12,
}: {
	words: TranscriptWord[];
	pauseSeconds?: number;
	maxSeconds?: number;
}): TimedLine[] {
	const sorted = [...words].filter((word) => word.text.trim()).sort((a, b) => a.start - b.start);
	const lines: TimedLine[] = [];
	let current: TranscriptWord[] = [];
	const flush = () => {
		if (current.length === 0) return;
		lines.push({
			text: current.map((word) => word.text.trim()).join(" "),
			start: current[0].start,
			end: Math.max(...current.map((word) => word.end)),
		});
		current = [];
	};
	for (const word of sorted) {
		const previous = current[current.length - 1];
		if (previous && (word.start - previous.end >= pauseSeconds || word.end - current[0].start > maxSeconds)) flush();
		current.push(word);
		if (SENTENCE_END.test(word.text.trim())) flush();
	}
	flush();
	return lines;
}

/** A clip's length from its first line's start to its last line's end. */
const span = (lines: TimedLine[], first: number, last: number) => lines[last].end - lines[first].start;

/**
 * The local pick, without Claude: every run of whole lines whose length
 * fits, scored by how much is said per second, with a bonus for a question
 * or exclamation at the start (a hook) and for ending on a sentence end;
 * the best that do not overlap, in timeline order.
 */
export function localPicks({ lines, options }: { lines: TimedLine[]; options: ShortsOptions }): ShortPick[] {
	const candidates: { first: number; last: number; score: number }[] = [];
	for (let first = 0; first < lines.length; first++) {
		let words = 0;
		for (let last = first; last < lines.length; last++) {
			words += lines[last].text.split(/\s+/).filter(Boolean).length;
			const length = span(lines, first, last);
			if (length > options.maxSeconds) break;
			if (length < options.minSeconds) continue;
			const hook = /[?!]/.test(lines[first].text) ? 1.25 : 1;
			const ending = SENTENCE_END.test(lines[last].text) ? 1.1 : 1;
			// Words per second, nudged towards longer clips so a short burst does not win.
			const score = (words / length) * hook * ending * (1 + Math.min(length, options.maxSeconds) / (4 * options.maxSeconds));
			candidates.push({ first, last, score });
		}
	}
	candidates.sort((a, b) => b.score - a.score || a.first - b.first);
	const kept: { first: number; last: number }[] = [];
	for (const candidate of candidates) {
		if (kept.length >= options.count) break;
		if (kept.some((other) => candidate.first <= other.last && candidate.last >= other.first)) continue;
		kept.push(candidate);
	}
	return kept
		.sort((a, b) => a.first - b.first)
		.map((pick, index) => ({
			first: pick.first,
			last: pick.last,
			start: lines[pick.first].start,
			end: lines[pick.last].end,
			title: titleFrom(lines[pick.first].text) || `Short ${index + 1}`,
			reason: "Picked locally: dense speech in a run of whole sentences.",
		}));
}

/** A line's opening sentence as a title when it is short, else its first few words. */
function titleFrom(text: string): string {
	const sentence = text.match(/^[^.!?。！？]*[?!]/u)?.[0] ?? text.match(/^[^.!?。！？]*/u)?.[0] ?? text;
	const sentenceWords = sentence.split(/\s+/).filter(Boolean);
	if (sentenceWords.length > 0 && sentenceWords.length <= 8) return sentence.trim();
	const words = text.replace(/[.!?。！？…,;:]+$/u, "").split(/\s+/).filter(Boolean);
	return words.slice(0, 6).join(" ") + (words.length > 6 ? "…" : "");
}

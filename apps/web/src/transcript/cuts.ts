/**
 * Text-based editing: deleting words from a clip's transcript cuts the
 * clip. `transcriptCuts` turns the deleted words into the timeline
 * stretches to remove, and `retimeCaptionTracks` keeps the captions in
 * step with the cut: words inside a removed stretch go, and with a ripple
 * over every track the words after it move left with the picture, even
 * inside a caption that straddles the cut (the ripple itself only moves
 * captions that start after it). The desktop twin is
 * `crates/editor/src/transcript.rs`.
 */

import type { CaptionWord } from "@/captions/types";
import type { RippleScope } from "@/silence/detect";
import type { SceneTracks, TextElement, TimelineElement, TimelineTrack } from "@/timeline/types";
import type { MediaTime } from "@/wasm";

export interface TranscriptWord {
	text: string;
	/** Seconds on the timeline. */
	start: number;
	end: number;
}

export interface SecondsRange {
	start: number;
	end: number;
}

export interface TickRange {
	start: MediaTime;
	end: MediaTime;
}

/** Seconds kept around a deleted word that has no neighbour on that side. */
export const TRANSCRIPT_EDGE_PADDING = 0.08;

/**
 * The stretches to cut for the deleted words (indices into `words`),
 * inside the clip's span `[from, to]`. Each run of deleted words is cut
 * from halfway through the pause before it to halfway through the pause
 * after it, so what is left keeps a natural pause; at the ends of the
 * transcript a little padding is cut instead.
 */
export function transcriptCuts({
	words,
	deleted,
	from,
	to,
}: {
	words: TranscriptWord[];
	deleted: Iterable<number>;
	from: number;
	to: number;
}): SecondsRange[] {
	const order = words.map((_, index) => index).sort((a, b) => words[a].start - words[b].start);
	const gone = new Set([...deleted].filter((index) => Number.isInteger(index) && index >= 0 && index < words.length));
	const cuts: SecondsRange[] = [];
	let index = 0;
	while (index < order.length) {
		if (!gone.has(order[index])) {
			index++;
			continue;
		}
		let last = index;
		while (last + 1 < order.length && gone.has(order[last + 1])) last++;
		const firstWord = words[order[index]];
		const lastWord = words[order[last]];
		const previous = index > 0 ? words[order[index - 1]] : null;
		const next = last + 1 < order.length ? words[order[last + 1]] : null;
		const start = previous
			? (Math.min(previous.end, firstWord.start) + firstWord.start) / 2
			: firstWord.start - TRANSCRIPT_EDGE_PADDING;
		const end = next ? (lastWord.end + Math.max(next.start, lastWord.end)) / 2 : lastWord.end + TRANSCRIPT_EDGE_PADDING;
		const clamped = { start: Math.max(start, from), end: Math.min(end, to) };
		if (clamped.end > clamped.start) {
			const previousCut = cuts[cuts.length - 1];
			if (previousCut && clamped.start <= previousCut.end) {
				previousCut.end = Math.max(previousCut.end, clamped.end);
			} else {
				cuts.push(clamped);
			}
		}
		index = last + 1;
	}
	return cuts;
}

function retimeCaption({
	element,
	cut,
	shifting,
}: {
	element: TextElement;
	cut: TickRange;
	shifting: boolean;
}): TextElement | null {
	const start = element.startTime;
	const end = element.startTime + element.duration;
	const shift = cut.end - cut.start;
	const newStart = shifting && start >= cut.start ? cut.start : start;
	const newEnd = end <= cut.end ? Math.max(cut.start, newStart) : shifting ? end - shift : end;
	const words: CaptionWord[] = [];
	const relative = (time: number) => Math.min(Math.max(time - newStart, 0), newEnd - newStart) as MediaTime;
	for (const word of element.words ?? []) {
		let wordStart = start + word.start;
		let wordEnd = start + word.end;
		const middle = Math.floor((wordStart + wordEnd) / 2);
		if (middle >= cut.start && middle < cut.end) continue;
		if (shifting && middle >= cut.end) {
			wordStart -= shift;
			wordEnd -= shift;
		} else if (shifting && middle < cut.start) {
			wordEnd = Math.min(wordEnd, cut.start);
		}
		words.push({ text: word.text, start: relative(wordStart), end: relative(wordEnd), ...(word.keyword && { keyword: true }) });
	}
	if (words.length === 0 || newEnd <= newStart) return null;
	return {
		...element,
		startTime: newStart as MediaTime,
		duration: (newEnd - newStart) as MediaTime,
		words,
		params: { ...element.params, content: words.map((word) => word.text.trim()).join(" ") },
	};
}

/**
 * The tracks with their timed captions brought in line with cutting
 * `ranges` (timeline ticks) out of the picture: a word whose middle falls
 * in a removed stretch is dropped, and with ripple `all` the words after
 * the stretch move left by its length. Captions left with no words go.
 * Captions entirely before or after every stretch are untouched (the
 * ripple moves the later ones), as are locked tracks and text without
 * word timings. Apply it before the cut, on the same ranges.
 */
export function retimeCaptionTracks({
	tracks,
	ranges,
	ripple,
}: {
	tracks: SceneTracks;
	ranges: TickRange[];
	ripple: RippleScope;
}): SceneTracks {
	const sorted = ranges.filter((range) => range.end > range.start).sort((a, b) => b.start - a.start);
	const shifting = ripple === "all";
	const retimeTrack = (track: TimelineTrack): TimelineTrack => {
		if (track.locked) return track;
		let elements = track.elements as TimelineElement[];
		let changed = false;
		// Latest first, so earlier stretches keep their positions.
		for (const cut of sorted) {
			const next: TimelineElement[] = [];
			for (const element of elements) {
				if (element.type !== "text" || !element.words?.length) {
					next.push(element);
					continue;
				}
				const end = element.startTime + element.duration;
				if (end <= cut.start || element.startTime >= cut.end) {
					next.push(element);
					continue;
				}
				changed = true;
				const retimed = retimeCaption({ element, cut, shifting });
				if (retimed) next.push(retimed);
			}
			elements = next;
		}
		if (!changed) return track;
		return { ...track, elements: [...elements].sort((a, b) => a.startTime - b.startTime) } as TimelineTrack;
	};
	return {
		overlay: tracks.overlay.map((track) => retimeTrack(track) as typeof track),
		main: retimeTrack(tracks.main) as typeof tracks.main,
		audio: tracks.audio.map((track) => retimeTrack(track) as typeof track),
	};
}

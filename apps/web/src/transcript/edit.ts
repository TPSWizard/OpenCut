/**
 * A clip's transcript (the timed words of the captions over it) and
 * cutting struck words out of it, with the captions kept in step.
 */

import { CutWithCaptionsCommand } from "@/commands/timeline/element/cut-with-captions";
import type { EditorCore } from "@/core";
import { wordsFromCaptions } from "@/fillers/remove";
import type { RippleScope } from "@/silence/detect";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import { transcriptCuts, type SecondsRange, type TranscriptWord } from "./cuts";

export interface ClipTranscript {
	/** The clip's span in timeline seconds. */
	from: number;
	to: number;
	words: TranscriptWord[];
}

export interface TranscriptCutResult {
	/** Timeline seconds cut out. */
	cuts: SecondsRange[];
	seconds: number;
	words: number;
	pieces: { trackId: string; elementId: string }[];
}

/** The clip's transcript, or null when the element is not an audio or video clip. */
export function clipTranscript({
	tracks,
	trackId,
	elementId,
}: {
	tracks: SceneTracks;
	trackId: string;
	elementId: string;
}): ClipTranscript | null {
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate: TimelineTrack) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || !(element.type === "video" || element.type === "audio")) return null;
	const from = element.startTime / TICKS_PER_SECOND;
	const to = (element.startTime + element.duration) / TICKS_PER_SECOND;
	// Only words that overlap the clip, not ones that just touch its edge.
	const words = wordsFromCaptions({ tracks, from, to }).filter((word) => word.end > from && word.start < to);
	return { from, to, words };
}

/** Cuts the deleted words (indices into the clip's transcript) out in one undo step. */
export function cutTranscriptWords({
	editor,
	trackId,
	elementId,
	deleted,
	ripple = "all",
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	deleted: Iterable<number>;
	ripple?: RippleScope;
}): TranscriptCutResult {
	const transcript = clipTranscript({ tracks: editor.scenes.getActiveScene().tracks, trackId, elementId });
	if (!transcript) throw new Error("Transcript editing needs an audio or video clip");
	const indices = [...new Set(deleted)];
	const cuts = transcriptCuts({ words: transcript.words, deleted: indices, from: transcript.from, to: transcript.to });
	const seconds = cuts.reduce((sum, cut) => sum + (cut.end - cut.start), 0);
	if (cuts.length === 0) return { cuts, seconds, words: 0, pieces: [{ trackId, elementId }] };
	const pieces = cutWithCaptions({ editor, trackId, elementId, ranges: cuts, ripple });
	return { cuts, seconds, words: indices.filter((index) => index >= 0 && index < transcript.words.length).length, pieces };
}

/** Cuts timeline-second ranges out of a clip with the captions retimed, in one undo step. */
export function cutWithCaptions({
	editor,
	trackId,
	elementId,
	ranges,
	ripple,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	ranges: SecondsRange[];
	ripple: RippleScope;
}): { trackId: string; elementId: string }[] {
	if (ranges.length === 0) return [{ trackId, elementId }];
	const command = new CutWithCaptionsCommand({
		trackId,
		elementId,
		ranges: ranges.map((range) => ({
			start: roundMediaTime({ time: range.start * TICKS_PER_SECOND }),
			end: roundMediaTime({ time: range.end * TICKS_PER_SECOND }),
		})),
		ripple,
	});
	const rippleMode = editor.command.isRippleEnabled;
	editor.command.isRippleEnabled = false;
	try {
		editor.command.execute({ command });
	} finally {
		editor.command.isRippleEnabled = rippleMode;
	}
	return command.getRemainingElements();
}

/**
 * The piece to keep editing after a cut: the first one starting at or after
 * `time` (where the first cut was), else the first piece.
 */
export function pieceAfter({
	tracks,
	pieces,
	time,
}: {
	tracks: SceneTracks;
	pieces: { trackId: string; elementId: string }[];
	time: number;
}): { trackId: string; elementId: string } | null {
	const all = [...tracks.overlay, tracks.main, ...tracks.audio];
	const starts = pieces
		.map((piece) => {
			const element = (all.find((track) => track.id === piece.trackId)?.elements as TimelineElement[] | undefined)?.find(
				(candidate) => candidate.id === piece.elementId,
			);
			return element ? { piece, start: element.startTime / TICKS_PER_SECOND } : null;
		})
		.filter((entry): entry is { piece: { trackId: string; elementId: string }; start: number } => entry !== null)
		.sort((a, b) => a.start - b.start);
	return (starts.find((entry) => entry.start >= time - 0.001) ?? starts[0])?.piece ?? null;
}

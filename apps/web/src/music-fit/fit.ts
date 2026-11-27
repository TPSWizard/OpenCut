/**
 * Fit music to a length (row 101): an audio clip is shortened by cutting
 * bars out, or lengthened by repeating bars, at bar lines where the music
 * sounds the same either side. The clip's playing span is decoded to mono
 * at the shared crate's rate and fitted by the Rust `musicfit` crate
 * through WASM (as the desktop does after FFmpeg decodes it); the clip
 * becomes pieces of the same source back to back on its track, each
 * crossfading into the next over 0.1 s (row 84's `crossfade` param, equal
 * power), in one undo step. The first piece keeps the clip's id.
 * Checked against `fixtures/music-fit/cases.json`.
 */

import { musicFit, musicFitConstants } from "opencut-wasm";
import { BatchCommand } from "@/commands";
import { InsertElementCommand } from "@/commands/timeline/element/insert-element";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { decodeAudioToFloat32 } from "@/media/audio";
import { allTracks } from "@/timeline/edits";
import { CROSSFADE_CURVE_PARAM, CROSSFADE_PARAM } from "@/timeline/audio-crossfade";
import type { CreateTimelineElement, TimelineElement } from "@/timeline";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";

export interface MusicFitConstants {
	sampleRate: number;
	crossfadeTicks: number;
	minSeconds: number;
	maxSeconds: number;
	maxPieces: number;
}

export interface MusicFitPiece {
	start: number;
	duration: number;
	trimStart: number;
	trimEnd: number;
	fadeIn: number;
	fadeOut: number;
	crossfade: number;
}

export interface MusicFitResult {
	kind: "cut" | "loop" | "trim";
	pieces: MusicFitPiece[];
	jumps: { from: number; to: number; fromSeconds: number; toSeconds: number; score: number }[];
	bpm: number | null;
	trimmed: number;
}

let constants: MusicFitConstants | null = null;

export function getMusicFitConstants(): MusicFitConstants {
	constants ??= JSON.parse(musicFitConstants()) as MusicFitConstants;
	return constants;
}

function isRetimed(element: TimelineElement): boolean {
	const retime = "retime" in element ? element.retime : undefined;
	if (!retime) return false;
	return retime.rate !== 1 || retime.reverse === true || (retime.ramp?.length ?? 0) > 0;
}

/** A clip music fitting works on: audio without a speed change. */
export function canFitMusic(element: TimelineElement | null | undefined): boolean {
	return element?.type === "audio" && !isRetimed(element);
}

function ownFades(element: TimelineElement): { fadeIn: number; fadeOut: number } {
	const finite = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
	const length = Math.max(0, element.duration / TICKS_PER_SECOND);
	let fadeIn = Math.min(10, Math.max(0, finite(element.params.fadeIn)));
	let fadeOut = Math.min(10, Math.max(0, finite(element.params.fadeOut)));
	if (fadeIn + fadeOut > length && fadeIn + fadeOut > 0) {
		const scale = length / (fadeIn + fadeOut);
		fadeIn *= scale;
		fadeOut *= scale;
	}
	return { fadeIn, fadeOut };
}

/** The clip as the shared crate takes it. */
export function musicFitClip(element: TimelineElement) {
	const { fadeIn, fadeOut } = ownFades(element);
	return {
		start: element.startTime,
		duration: element.duration,
		trimStart: element.trimStart,
		trimEnd: element.trimEnd,
		fadeIn,
		fadeOut,
	};
}

function find(editor: EditorCore, elementId: string, trackId?: string) {
	for (const track of allTracks({ tracks: editor.scenes.getActiveScene().tracks })) {
		if (trackId && track.id !== trackId) continue;
		const element = (track.elements as TimelineElement[]).find((candidate) => candidate.id === elementId);
		if (element) return { track, element };
	}
	return null;
}

/**
 * The length (ticks) the music needs to end with everything else on the
 * timeline: from its start to the latest end of any other element; `null`
 * when nothing else ends a second or more after it starts.
 */
export function musicFitDefaultLength(editor: EditorCore, elementId: string): number | null {
	const found = find(editor, elementId);
	if (!found) return null;
	let end = -Infinity;
	for (const track of allTracks({ tracks: editor.scenes.getActiveScene().tracks })) {
		for (const other of track.elements as TimelineElement[]) {
			if (other.id !== elementId) end = Math.max(end, other.startTime + other.duration);
		}
	}
	const length = end - found.element.startTime;
	return Number.isFinite(length) && length >= TICKS_PER_SECOND ? length : null;
}

async function clipBlob(editor: EditorCore, element: TimelineElement): Promise<Blob> {
	const mediaId = (element as { mediaId?: string }).mediaId;
	const asset = mediaId ? editor.media.getAssets().find((candidate) => candidate.id === mediaId) : undefined;
	if (asset?.file) return asset.file;
	const sourceUrl = (element as { sourceUrl?: string }).sourceUrl;
	if (sourceUrl) {
		const response = await fetch(sourceUrl);
		if (response.ok) return await response.blob();
	}
	throw new Error(`${element.name}'s media file is not available`);
}

/** Fits the clip's music to `length` ticks (no change to the timeline). */
export async function planMusicFit({
	editor,
	element,
	length,
}: {
	editor: EditorCore;
	element: TimelineElement;
	length: number;
}): Promise<MusicFitResult> {
	const { sampleRate } = getMusicFitConstants();
	let decoded: { samples: Float32Array; sampleRate: number };
	try {
		decoded = await decodeAudioToFloat32({ audioBlob: await clipBlob(editor, element), sampleRate });
	} catch (error) {
		if (error instanceof Error && error.message.endsWith("is not available")) throw error;
		throw new Error(`${element.name} has no sound that can be read`);
	}
	const first = Math.max(0, Math.round((element.trimStart / TICKS_PER_SECOND) * decoded.sampleRate));
	const count = Math.round((element.duration / TICKS_PER_SECOND) * decoded.sampleRate);
	const span = decoded.samples.subarray(first, Math.min(decoded.samples.length, first + count));
	if (span.length === 0) throw new Error("No audio could be read from the clip");
	let json: string;
	try {
		json = musicFit(span, decoded.sampleRate, JSON.stringify(musicFitClip(element)), Math.round(length));
	} catch (error) {
		throw new Error(typeof error === "string" ? error : error instanceof Error ? error.message : String(error));
	}
	return JSON.parse(json) as MusicFitResult;
}

/**
 * Fits an audio clip to `seconds` (or, without, to end with everything
 * else on the timeline) and replaces it with the pieces in one undo step;
 * the first piece stays selected.
 */
export async function fitMusic({
	editor,
	elementId,
	trackId,
	seconds,
}: {
	editor: EditorCore;
	elementId: string;
	trackId?: string;
	seconds?: number;
}): Promise<{ trackId: string; elementId: string; fit: MusicFitResult; length: number }> {
	const found = find(editor, elementId, trackId);
	if (!found) throw new Error("The clip is no longer on the timeline");
	if (!canFitMusic(found.element)) throw new Error("Fit to length works on audio clips without a speed change");
	let length: number;
	if (seconds === undefined) {
		const fallback = musicFitDefaultLength(editor, elementId);
		if (fallback === null) throw new Error("Nothing else on the timeline to fit the music to; type a length instead");
		length = fallback;
	} else {
		if (!Number.isFinite(seconds) || seconds <= 0) throw new Error("Type a length in seconds");
		length = Math.round(seconds * TICKS_PER_SECOND);
	}
	const fit = await planMusicFit({ editor, element: found.element, length });
	// The timeline may have changed while the music was analysed.
	const current = find(editor, elementId, trackId);
	if (!current) throw new Error("The clip is no longer on the timeline");
	const { track, element } = current;
	if (track.locked) throw new Error("The track is locked");
	const last = fit.pieces[fit.pieces.length - 1];
	const end = last.start + last.duration;
	const blocked = (track.elements as TimelineElement[]).some(
		(other) => other.id !== elementId && other.startTime < end && other.startTime + other.duration > element.startTime,
	);
	if (blocked) throw new Error("Another clip on this track is in the way; move it or put the music on its own track");
	const pieceParams = (piece: MusicFitPiece) => ({
		fadeIn: piece.fadeIn,
		fadeOut: piece.fadeOut,
		...(piece.crossfade > 0
			? { [CROSSFADE_PARAM]: piece.crossfade / TICKS_PER_SECOND, [CROSSFADE_CURVE_PARAM]: "equalPower" }
			: {}),
	});
	const [head, ...rest] = fit.pieces;
	const commands = [
		new UpdateElementsCommand({
			updates: [
				{
					trackId: track.id,
					elementId,
					patch: {
						startTime: roundMediaTime({ time: head.start }),
						duration: roundMediaTime({ time: head.duration }),
						trimStart: roundMediaTime({ time: head.trimStart }),
						trimEnd: roundMediaTime({ time: head.trimEnd }),
						params: { ...element.params, ...pieceParams(head) },
					} as Partial<TimelineElement>,
				},
			],
		}),
		...rest.map((piece) => {
			const { id: _id, ...copy } = element;
			const created = {
				...copy,
				startTime: roundMediaTime({ time: piece.start }),
				duration: roundMediaTime({ time: piece.duration }),
				trimStart: roundMediaTime({ time: piece.trimStart }),
				trimEnd: roundMediaTime({ time: piece.trimEnd }),
				// The last piece keeps any crossfade the clip had into what follows.
				params: { ...element.params, ...pieceParams(piece) },
			} as unknown as CreateTimelineElement;
			return new InsertElementCommand({ element: created, placement: { mode: "explicit", trackId: track.id } });
		}),
	];
	const rippleMode = editor.command.isRippleEnabled;
	editor.command.isRippleEnabled = false;
	try {
		editor.command.execute({ command: new BatchCommand(commands) });
	} finally {
		editor.command.isRippleEnabled = rippleMode;
	}
	editor.selection.setSelectedElements({ elements: [{ trackId: track.id, elementId }] });
	return { trackId: track.id, elementId, fit, length };
}

/** What fitting did, in words (English; the section translates its parts). */
export function musicFitSummary(fit: MusicFitResult, length: number): { what: "cut" | "loop" | "trim"; joins: number; seconds: number; trimmed: number } {
	return { what: fit.kind, joins: fit.jumps.length, seconds: length / TICKS_PER_SECOND, trimmed: fit.trimmed };
}

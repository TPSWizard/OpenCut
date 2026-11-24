/**
 * Syncing multicam angles in the web editor: each selected angle's sound
 * over the part of its file it plays is decoded (mono, 8 kHz), turned into
 * onsets and lined up with the first angle's by the shared `multicam`
 * crate (WASM), as the desktop does (`apps/desktop/src/multicam.rs`); then
 * the angles move and group in one undo step.
 */

import { multicamMinSyncScore, multicamOnsets, multicamSyncLag } from "opencut-wasm";
import type { EditorCore } from "@/core";
import { decodeAudioToFloat32 } from "@/media/audio";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { multicamSyncCommand } from "./group";

export const SYNC_RATE = 8000;
export const SYNC_MAX_SECONDS = 600;

export interface ElementRef {
	trackId?: string;
	elementId: string;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** Lines the angles up (the first stays put) and groups them; returns each angle's lag. */
export async function syncMulticam({
	editor,
	elements,
	onProgress,
}: {
	editor: EditorCore;
	elements: ElementRef[];
	onProgress?: (done: number, total: number) => void;
}): Promise<{ lags: { elementId: string; name: string; lag: number; score: number }[] }> {
	if (elements.length < 2) throw new Error("Select two or more video clips to sync");
	const tracks = editor.scenes.getActiveScene().tracks;
	const found = elements.map((ref) => {
		for (const track of allTracks(tracks)) {
			if (ref.trackId && track.id !== ref.trackId) continue;
			const element = (track.elements as TimelineElement[]).find((candidate) => candidate.id === ref.elementId);
			if (element) return element;
		}
		throw new Error("A clip to sync is no longer on the timeline");
	});
	const onsets: Float64Array[] = [];
	const lengths: number[] = [];
	for (const [index, element] of found.entries()) {
		if (element.type !== "video") throw new Error("Multicam angles must be video clips with sound");
		const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
		if (!asset?.file) throw new Error(`${element.name}'s media file is not available`);
		const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
		const length = seconds(element.duration);
		const sourceAt = (clipTime: number) =>
			seconds(element.trimStart) + getSourceTimeAtClipTime({ clipTime, retime: element.retime, duration: length });
		const a = sourceAt(0);
		const b = sourceAt(Math.max(0, length - 1 / TICKS_PER_SECOND));
		const from = Math.max(0, Math.min(a, b));
		const span = Math.min(Math.abs(a - b), SYNC_MAX_SECONDS);
		let decoded: { samples: Float32Array; sampleRate: number };
		try {
			decoded = await decodeAudioToFloat32({ audioBlob: asset.file, sampleRate: SYNC_RATE });
		} catch {
			throw new Error(`${element.name} has no sound that can be read`);
		}
		const first = Math.floor(from * decoded.sampleRate);
		const samples = decoded.samples.subarray(first, Math.min(decoded.samples.length, first + Math.floor(span * decoded.sampleRate)));
		if (samples.length < decoded.sampleRate) throw new Error(`${element.name} has less than a second of sound`);
		onsets.push(multicamOnsets(samples, decoded.sampleRate));
		lengths.push(span);
		onProgress?.(index + 1, found.length);
	}
	const maxLag = Math.max(...lengths);
	const lags = [{ elementId: found[0].id, name: found[0].name, lag: 0, score: 1 }];
	for (let index = 1; index < found.length; index++) {
		const result = multicamSyncLag(onsets[0], onsets[index], maxLag);
		if (result.length < 2) throw new Error(`${found[index].name} could not be compared`);
		if (result[1] < multicamMinSyncScore()) {
			throw new Error(`${found[index].name}'s sound does not match the first clip's well enough to sync`);
		}
		lags.push({ elementId: found[index].id, name: found[index].name, lag: result[0], score: result[1] });
	}
	editor.command.execute({
		command: multicamSyncCommand({ tracks: editor.scenes.getActiveScene().tracks, lags }),
	});
	return { lags };
}

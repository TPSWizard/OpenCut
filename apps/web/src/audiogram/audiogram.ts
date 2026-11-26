/**
 * Audiograms (row 100): a layer of bars, mirrored bars, a line or a circle
 * that moves with a clip's sound. The clip's sound is decoded to mono at
 * the shared crate's rate and analysed once per frame by the Rust
 * `audiogram` crate through WASM (as the desktop does after FFmpeg
 * decodes it); the levels are kept on a graphic element (definition
 * "audiogram", the `levels` param) that the same crate draws each frame.
 * Checked against `fixtures/audiogram/cases.json`.
 */

import { audiogramAnalyse, audiogramConstants, audiogramStyles } from "opencut-wasm";
import type { EditorCore } from "@/core";
import { decodeAudioToFloat32 } from "@/media/audio";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import { allTracks } from "@/timeline/edits";
import type { TimelineElement } from "@/timeline";
import { buildGraphicElement } from "@/timeline/element-utils";
import { TICKS_PER_SECOND, mediaTimeFromSeconds, type MediaTime } from "@/wasm";

/** The graphic definition id (`audiogram::DEFINITION`). */
export const AUDIOGRAM_DEFINITION = "audiogram";

export interface AudiogramConstants {
	sampleRate: number;
	fps: number;
	bands: number;
	maxSeconds: number;
	maxRasterSide: number;
}

let constants: AudiogramConstants | null = null;
let styles: { styles: { id: string; label: string }[]; defaults: Record<string, string | number> } | null = null;

export function getAudiogramConstants(): AudiogramConstants {
	constants ??= JSON.parse(audiogramConstants()) as AudiogramConstants;
	return constants;
}

/** The styles and the look's default params. */
export function getAudiogramStyles(): { styles: { id: string; label: string }[]; defaults: Record<string, string | number> } {
	styles ??= JSON.parse(audiogramStyles());
	return styles!;
}

export function isAudiogram(element: TimelineElement | null | undefined): boolean {
	return element?.type === "graphic" && (element as { definitionId?: string }).definitionId === AUDIOGRAM_DEFINITION;
}

export function canMakeAudiogram(element: TimelineElement | null | undefined): boolean {
	return element?.type === "video" || element?.type === "audio";
}

/**
 * The source second each level frame looks at for a clip `duration`
 * seconds long, `trimStart` into its source, with its speed change.
 */
export function audiogramSourceTimes({
	trimStart,
	duration,
	retime,
}: {
	trimStart: number;
	duration: number;
	retime?: Parameters<typeof getSourceTimeAtClipTime>[0]["retime"];
}): Float64Array {
	const { fps, maxSeconds } = getAudiogramConstants();
	const count = Math.max(1, Math.ceil(Math.min(duration, maxSeconds) * fps - 1e-9));
	const last = Math.max(0, duration - 1 / TICKS_PER_SECOND);
	const times = new Float64Array(count);
	for (let frame = 0; frame < count; frame++) {
		const clipTime = Math.min(frame / fps, last);
		times[frame] = trimStart + getSourceTimeAtClipTime({ clipTime, retime, duration });
	}
	return times;
}

/**
 * Makes an audiogram of a video or audio clip: listens to its sound, then
 * puts the audiogram over the clip's span and selects it (one undo step).
 * Returns the new element's track and id.
 */
export async function makeAudiogram({
	editor,
	trackId,
	elementId,
}: {
	editor: EditorCore;
	trackId?: string;
	elementId: string;
}): Promise<{ trackId: string; elementId: string }> {
	const tracks = editor.scenes.getActiveScene().tracks;
	let source: TimelineElement | undefined;
	for (const track of allTracks({ tracks })) {
		if (trackId && track.id !== trackId) continue;
		source = (track.elements as TimelineElement[]).find((candidate) => candidate.id === elementId);
		if (source) break;
	}
	if (!source) throw new Error("The clip is no longer on the timeline");
	if (!canMakeAudiogram(source)) throw new Error("Select a video or audio clip with sound to make an audiogram");
	const mediaId = (source as { mediaId?: string }).mediaId;
	const asset = editor.media.getAssets().find((candidate) => candidate.id === mediaId);
	if (!asset?.file) throw new Error(`${source.name}'s media file is not available`);
	const { sampleRate, fps } = getAudiogramConstants();
	let decoded: { samples: Float32Array; sampleRate: number };
	try {
		decoded = await decodeAudioToFloat32({ audioBlob: asset.file, sampleRate });
	} catch {
		throw new Error(`${source.name} has no sound that can be read`);
	}
	const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
	const times = audiogramSourceTimes({
		trimStart: seconds(source.trimStart),
		duration: seconds(source.duration),
		retime: (source as { retime?: Parameters<typeof getSourceTimeAtClipTime>[0]["retime"] }).retime,
	});
	const levels = audiogramAnalyse(decoded.samples, decoded.sampleRate, times);
	const length = Math.min(seconds(source.duration), times.length / fps);
	const { defaults } = getAudiogramStyles();
	const element = {
		...buildGraphicElement({
			definitionId: AUDIOGRAM_DEFINITION,
			name: "Audiogram",
			startTime: source.startTime,
			params: { ...defaults, levels, sourceName: source.name },
		}),
		duration: mediaTimeFromSeconds({ seconds: length }) as MediaTime,
	};
	editor.timeline.insertElement({ element, placement: { mode: "auto", trackType: "graphic" } });
	for (const track of allTracks({ tracks: editor.scenes.getActiveScene().tracks })) {
		const found = (track.elements as TimelineElement[]).find(
			(candidate) => isAudiogram(candidate) && candidate.params.levels === levels && candidate.startTime === source.startTime,
		);
		if (found) {
			editor.selection.setSelectedElements({ elements: [{ trackId: track.id, elementId: found.id }] });
			return { trackId: track.id, elementId: found.id };
		}
	}
	throw new Error("The audiogram could not be added");
}

/**
 * Auto zoom on a clip: the words of the captions over it become punch-ins
 * (`planPunches`), optionally centred on the speaker's face (MediaPipe on a
 * few frames, as auto-reframe uses), written to the clip's `zoom.*` params
 * in one undo step.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { FACE_MIN_SCORE, subjectFromDetections, type Subject } from "@/reframe/path";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import {
	ZOOM_FOCUS_X_PARAM,
	ZOOM_FOCUS_Y_PARAM,
	ZOOM_PUNCHES_PARAM,
	formatPunches,
	planPunches,
	type ZoomPunch,
	type ZoomStrength,
	type ZoomWord,
} from "./zoom";

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

function findClip({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): TimelineElement {
	const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || (element.type !== "video" && element.type !== "image")) throw new Error("Auto zoom needs a video or picture clip");
	return element;
}

/** The words of the captions over the clip, in its seconds (`clip_words` in the Rust editor). */
export function clipZoomWords({ editor, element }: { editor: EditorCore; element: TimelineElement }): ZoomWord[] {
	const start = element.startTime;
	const end = element.startTime + element.duration;
	const words: ZoomWord[] = [];
	for (const track of allTracks(editor.scenes.getActiveScene().tracks)) {
		for (const caption of track.elements as TimelineElement[]) {
			if (caption.type !== "text" || !caption.words?.length) continue;
			if (caption.startTime + caption.duration <= start || caption.startTime >= end) continue;
			const offset = (caption.startTime - start) / TICKS_PER_SECOND;
			for (const word of caption.words) {
				words.push({
					text: word.text,
					start: offset + word.start / TICKS_PER_SECOND,
					end: offset + word.end / TICKS_PER_SECOND,
					keyword: word.keyword,
				});
			}
		}
	}
	return words;
}

/** Frames looked at for the face. */
const FACE_SAMPLES = 8;

/**
 * Where the speaker's face sits in a video clip, as a share of its frame
 * from the centre (the median over a few frames), or null without faces.
 */
export async function detectFaceFocus({
	editor,
	element,
	detectFaces,
}: {
	editor: EditorCore;
	element: TimelineElement;
	/** Injected for tests; defaults to MediaPipe. */
	detectFaces?: typeof import("@/reframe/faces").detectFaces;
}): Promise<{ x: number; y: number } | null> {
	if (element.type !== "video") return null;
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");
	const detector = detectFaces ?? (await import("@/reframe/faces")).detectFaces;
	const clipSeconds = element.duration / TICKS_PER_SECOND;
	const trim = element.trimStart / TICKS_PER_SECOND;
	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	const found: Subject[] = [];
	try {
		const video = await input.getPrimaryVideoTrack();
		if (!video || !(await video.canDecode())) return null;
		const width = 320;
		const height = Math.max(2, Math.round((width * (video.displayHeight || 1080)) / (video.displayWidth || 1920) / 2) * 2);
		const sink = new CanvasSink(video, { width, height, fit: "fill" });
		const times = Array.from({ length: FACE_SAMPLES }, (_, index) =>
			trim + getSourceTimeAtClipTime({ clipTime: ((index + 0.5) / FACE_SAMPLES) * clipSeconds, retime: element.retime, duration: clipSeconds }),
		);
		for await (const wrapped of sink.canvasesAtTimestamps(times)) {
			if (!wrapped) continue;
			const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			const subject = subjectFromDetections({ detections: await detector({ image: canvas, width, height }), minScore: FACE_MIN_SCORE });
			if (subject) found.push(subject);
		}
	} finally {
		input.dispose();
	}
	if (found.length === 0) return null;
	const median = (values: number[]) => values.sort((a, b) => a - b)[Math.floor(values.length / 2)];
	return { x: median(found.map((subject) => subject.x)) - 0.5, y: median(found.map((subject) => subject.y)) - 0.5 };
}

export interface AutoZoomResult {
	punches: ZoomPunch[];
	/** Words the plan was made from. */
	words: number;
	/** The focus written, when a face was looked for and found. */
	focus: { x: number; y: number } | null;
}

/** Plans punch-ins from the captions over the clip and writes them (one undo step). */
export async function autoZoomClip({
	editor,
	trackId,
	elementId,
	strength = "medium",
	findFace = true,
	detectFaces,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	strength?: ZoomStrength;
	findFace?: boolean;
	detectFaces?: typeof import("@/reframe/faces").detectFaces;
}): Promise<AutoZoomResult> {
	const element = findClip({ editor, trackId, elementId });
	const words = clipZoomWords({ editor, element });
	if (words.length === 0) throw new Error("No captions with word timings over this clip; generate captions first");
	const punches = planPunches({ words, duration: element.duration / TICKS_PER_SECOND, strength });
	if (punches.length === 0) throw new Error("The captions over this clip are too short for a punch-in");
	const focus = findFace ? await detectFaceFocus({ editor, element, detectFaces }) : null;
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId,
					patch: {
						params: {
							...element.params,
							[ZOOM_PUNCHES_PARAM]: formatPunches(punches),
							...(focus && {
								[ZOOM_FOCUS_X_PARAM]: Math.round(focus.x * 1000) / 1000,
								[ZOOM_FOCUS_Y_PARAM]: Math.round(focus.y * 1000) / 1000,
							}),
						},
					} as Partial<TimelineElement>,
				},
			],
		}),
	});
	return { punches, words: words.length, focus };
}

/** Removes the clip's punch-ins (one undo step). */
export function clearAutoZoom({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): void {
	const element = findClip({ editor, trackId, elementId });
	const params = { ...element.params };
	delete params[ZOOM_PUNCHES_PARAM];
	editor.command.execute({
		command: new UpdateElementsCommand({ updates: [{ trackId, elementId, patch: { params } as Partial<TimelineElement> }] }),
	});
}

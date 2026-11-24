/**
 * Face-only retouch in the web editor: a video's played part (5 looks a
 * second) or a picture is searched for faces with MediaPipe's BlazeFace
 * (`@/reframe/faces`, served from this site); the boxes become the clip's
 * face track (`faces.track`, the shared `facetrack` crate through WASM) and
 * its Retouch effect is set to "faces only" (added at its defaults when it
 * has none), in one undo step. The desktop finds faces with FaceFusion's
 * model and builds the same change (`editor::face_retouch_commands`).
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { faceTrackEncode, faceTrackFoundCount } from "opencut-wasm";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { buildDefaultEffectInstance } from "@/effects";
import { detectFaces } from "@/reframe/faces";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { FACE_SAMPLE_RATE, FACE_TRACK_PARAM } from "./track";

/** Frames are searched at this width. */
const LOOK_WIDTH = 640;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

type Box = [number, number, number, number];

async function look(image: HTMLCanvasElement | OffscreenCanvas | ImageBitmap, width: number, height: number): Promise<Box[]> {
	const found = await detectFaces({ image, width, height });
	return found.map((face) => [face.x + face.width / 2, face.y + face.height / 2, face.width, face.height]);
}

/** Finds the clip's faces and sets its Retouch to faces only; returns how many looks found one. */
export async function retouchFaces({
	editor,
	trackId,
	elementId,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	onProgress?: (fraction: number) => void;
}): Promise<{ looks: number; found: number }> {
	const find = () => {
		const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!element) throw new Error("The clip is no longer on the timeline");
		if (element.type !== "video" && element.type !== "image") throw new Error("Face retouch works on video and picture clips");
		return element;
	};
	const element = find();
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");

	let start = 0;
	const frames: Box[][] = [];
	if (element.type === "image") {
		const bitmap = await createImageBitmap(asset.file);
		const scale = Math.min(1, LOOK_WIDTH / bitmap.width);
		const canvas = new OffscreenCanvas(Math.round(bitmap.width * scale), Math.round(bitmap.height * scale));
		canvas.getContext("2d")?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
		bitmap.close();
		frames.push(await look(canvas, canvas.width, canvas.height));
	} else {
		const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
		const length = seconds(element.duration);
		const sourceAt = (clipTime: number) =>
			seconds(element.trimStart) + getSourceTimeAtClipTime({ clipTime, retime: element.retime, duration: length });
		const a = sourceAt(0);
		const b = sourceAt(Math.max(0, length - 1 / TICKS_PER_SECOND));
		start = Math.max(0, Math.min(a, b));
		const span = Math.abs(a - b);
		const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
		try {
			const video = await input.getPrimaryVideoTrack();
			if (!video || !(await video.canDecode())) throw new Error("The video cannot be decoded in this browser");
			const width = Math.min(LOOK_WIDTH, video.displayWidth || LOOK_WIDTH);
			const height = Math.max(2, Math.round((width * (video.displayHeight || 360)) / (video.displayWidth || 640) / 2) * 2);
			const sink = new CanvasSink(video, { width, height, fit: "fill" });
			const times: number[] = [];
			for (let index = 0; index <= Math.floor(span * FACE_SAMPLE_RATE); index++) times.push(start + index / FACE_SAMPLE_RATE + 1e-3);
			for await (const wrapped of sink.canvasesAtTimestamps(times)) {
				frames.push(wrapped ? await look(wrapped.canvas as HTMLCanvasElement | OffscreenCanvas, width, height) : []);
				onProgress?.(frames.length / times.length);
			}
		} finally {
			input.dispose();
		}
	}
	const track = faceTrackEncode(asset.id, start, FACE_SAMPLE_RATE, JSON.stringify(frames));
	const found = faceTrackFoundCount(track);
	if (found === 0) throw new Error("No faces were found in the clip");

	const current = find();
	const effects = [...(current.effects ?? [])];
	const index = effects.findIndex((effect) => effect.type === "retouch");
	if (index >= 0) {
		effects[index] = { ...effects[index], enabled: true, params: { ...effects[index].params, area: "faces" } };
	} else {
		const added = buildDefaultEffectInstance({ effectType: "retouch" });
		effects.push({ ...added, params: { ...added.params, area: "faces" } });
	}
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId,
					patch: { params: { ...current.params, [FACE_TRACK_PARAM]: track }, effects } as Partial<TimelineElement>,
				},
			],
		}),
	});
	return { looks: frames.length, found };
}

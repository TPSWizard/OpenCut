/**
 * Measuring a video clip for stabilise in the web editor (row 107): the
 * part of the file the clip plays is read with mediabunny 192 pixels wide,
 * at the file's own frame rate (10 to 60 a second, else 30) from where the
 * clip starts, as FFmpeg reads it on the desktop
 * (`apps/desktop/src/stabilise.rs`), and the camera's movement between each
 * pair of frames is measured with the shared `stabilise` crate (WASM).
 * Setting a strength measures first when the clip has no camera path from
 * its file, in one undo step with the strength.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { stabiliseEncodePath, stabiliseMeasureRate, stabiliseMeasureSize, stabiliseStep } from "opencut-wasm";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import {
	STABILISE_PATH_PARAM,
	STABILISE_STRENGTH_PARAM,
	type StabiliseStrength,
	readStabilisePath,
	stabiliseZoom,
} from "./steady";

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** The camera path of `file` from source second `from` to `to`, as the clip stores it. */
export async function measureCameraPath({
	file,
	mediaId,
	from,
	to,
	onProgress,
}: {
	file: Blob;
	mediaId: string;
	from: number;
	to: number;
	onProgress?: (fraction: number) => void;
}): Promise<{ text: string; frames: number; unmeasured: number }> {
	const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track || !(await track.canDecode())) throw new Error("The video cannot be decoded in this browser");
		const sourceWidth = track.displayWidth || 1920;
		const sourceHeight = track.displayHeight || 1080;
		const stats = await track.computePacketStats(120).catch(() => null);
		const rate = stabiliseMeasureRate(stats?.averagePacketRate ?? 0);
		const [width, height] = stabiliseMeasureSize(sourceWidth, sourceHeight);
		const sink = new CanvasSink(track, { width, height, fit: "fill" });
		const times: number[] = [];
		for (let index = 0; from + index / rate <= to + 0.5 / rate; index++) {
			// A hair past the frame time, so a frame starting exactly there is the one read.
			times.push(from + index / rate + 1e-3);
		}
		const steps: number[] = [];
		let previous: Uint8Array | null = null;
		let frames = 0;
		let unmeasured = 0;
		let lastTimestamp: number | null = null;
		let start: number | null = null;
		let read = 0;
		for await (const wrapped of sink.canvasesAtTimestamps(times)) {
			read++;
			onProgress?.(read / times.length);
			// A time past the last frame adds nothing.
			if (!wrapped) continue;
			// The same frame again (variable frame rate): no movement, so the
			// path stays one entry a measured time.
			if (wrapped.timestamp === lastTimestamp && previous) {
				steps.push(1, 0, 0, 0);
				frames++;
				continue;
			}
			lastTimestamp = wrapped.timestamp;
			const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			const context = canvas.getContext("2d", { willReadFrequently: true }) as
				| CanvasRenderingContext2D
				| OffscreenCanvasRenderingContext2D
				| null;
			if (!context) continue;
			const pixels = new Uint8Array(context.getImageData(0, 0, width, height).data.buffer);
			start ??= wrapped.timestamp;
			if (previous) {
				const step = stabiliseStep(previous, pixels, width, height);
				if (step.length === 4) steps.push(step[0], step[1], step[2], step[3]);
				else {
					steps.push(Number.NaN, Number.NaN, Number.NaN, Number.NaN);
					unmeasured++;
				}
			}
			previous = pixels;
			frames++;
		}
		if (frames === 0 || start === null) throw new Error("No frames could be read from the clip");
		return {
			text: stabiliseEncodePath(mediaId, start, rate, sourceHeight / sourceWidth, new Float64Array(steps)),
			frames,
			unmeasured,
		};
	} finally {
		input.dispose();
	}
}

/**
 * Sets stabilise on a video clip, measuring it first when it has no camera
 * path from its file. Returns how many frames the path holds and the zoom
 * used to hide the edges.
 */
export async function setStabilise({
	editor,
	trackId,
	elementId,
	strength,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	strength: StabiliseStrength;
	onProgress?: (fraction: number) => void;
}): Promise<{ strength: StabiliseStrength; frames: number; measured: boolean; zoom: number; edgesShow: boolean }> {
	const find = () => {
		const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!element) throw new Error("The clip is no longer on the timeline");
		if (element.type !== "video") throw new Error("Stabilise works on video clips");
		return element as VideoElement;
	};
	const element = find();
	const path = readStabilisePath(element.params, element.mediaId);
	let text: string | null = null;
	let frames = path?.frames() ?? 0;
	if (strength !== "off" && !path) {
		const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
		if (!asset?.file) throw new Error("The video's media file is not available");
		const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
		const length = seconds(element.duration);
		const sourceAt = (clipTime: number) =>
			seconds(element.trimStart) + getSourceTimeAtClipTime({ clipTime, retime: element.retime, duration: length });
		const a = sourceAt(0);
		const b = sourceAt(Math.max(0, length - 1 / TICKS_PER_SECOND));
		const measured = await measureCameraPath({
			file: asset.file,
			mediaId: element.mediaId,
			from: Math.max(0, Math.min(a, b)),
			to: Math.max(a, b),
			onProgress,
		});
		text = measured.text;
		frames = measured.frames;
	}
	// The clip may have changed while it was measured.
	const current = find();
	const params = {
		...current.params,
		[STABILISE_STRENGTH_PARAM]: strength,
		...(text !== null && { [STABILISE_PATH_PARAM]: text }),
	};
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [{ trackId, elementId, patch: { params } as Partial<TimelineElement> }],
		}),
	});
	const zoom = stabiliseZoom(params, current.mediaId);
	return {
		strength,
		frames,
		measured: text !== null,
		zoom: zoom?.zoom ?? 1,
		edgesShow: zoom ? zoom.needed > zoom.zoom + 1e-6 : false,
	};
}

/**
 * Measuring a video clip for deflicker in the web editor: the part of the
 * file the clip plays is read with mediabunny at 160 pixels wide, 30 frames
 * a second from where the clip starts (as FFmpeg's `fps` filter does on the
 * desktop, `apps/desktop/src/deflicker.rs`), and each frame's level is
 * measured with the shared `deflicker` crate (WASM). Setting a strength
 * measures first when the clip has no levels from its file, in one undo
 * step with the strength.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { deflickerEncodeLevels, deflickerFrameLevel } from "opencut-wasm";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import {
	DEFLICKER_LEVELS_PARAM,
	DEFLICKER_STRENGTH_PARAM,
	type DeflickerStrength,
	readDeflickerLevels,
} from "./levels";

/** Frames are measured this wide, this many a second (`deflicker::MEASURE_*`). */
export const MEASURE_WIDTH = 160;
export const MEASURE_FPS = 30;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** The levels of `file` from source second `from` to `to`, as the clip stores them. */
export async function measureLevels({
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
}): Promise<{ text: string; frames: number }> {
	const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track || !(await track.canDecode())) throw new Error("The video cannot be decoded in this browser");
		const sourceWidth = track.displayWidth || 1920;
		const sourceHeight = track.displayHeight || 1080;
		const width = MEASURE_WIDTH;
		const height = Math.max(2, Math.round((MEASURE_WIDTH * sourceHeight) / sourceWidth / 2) * 2);
		const sink = new CanvasSink(track, { width, height, fit: "fill" });
		const times: number[] = [];
		for (let index = 0; from + index / MEASURE_FPS <= to + 0.5 / MEASURE_FPS; index++) {
			// A hair past the frame time, so a frame starting exactly there is the one read.
			times.push(from + index / MEASURE_FPS + 1e-3);
		}
		const values: number[] = [];
		let last: number | null = null;
		for await (const wrapped of sink.canvasesAtTimestamps(times)) {
			let level: number | null = last;
			if (wrapped) {
				const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
				const context = canvas.getContext("2d", { willReadFrequently: true }) as
					| CanvasRenderingContext2D
					| OffscreenCanvasRenderingContext2D
					| null;
				if (context) {
					level = deflickerFrameLevel(new Uint8Array(context.getImageData(0, 0, width, height).data.buffer), width, height);
				}
			}
			// A time past the last frame repeats the one before.
			if (level === null) continue;
			values.push(level);
			last = level;
			onProgress?.(values.length / times.length);
		}
		if (values.length === 0) throw new Error("No frames could be read from the clip");
		return { text: deflickerEncodeLevels(mediaId, from, MEASURE_FPS, new Float64Array(values)), frames: values.length };
	} finally {
		input.dispose();
	}
}

/**
 * Sets deflicker on a video clip, measuring it first when it has no levels
 * from its file. Returns how many frames the levels hold.
 */
export async function setDeflicker({
	editor,
	trackId,
	elementId,
	strength,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	strength: DeflickerStrength;
	onProgress?: (fraction: number) => void;
}): Promise<{ strength: DeflickerStrength; frames: number; measured: boolean }> {
	const find = () => {
		const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!element) throw new Error("The clip is no longer on the timeline");
		if (element.type !== "video") throw new Error("Deflicker works on video clips");
		return element as VideoElement;
	};
	const element = find();
	const levels = readDeflickerLevels(element.params, element.mediaId);
	let text: string | null = null;
	let frames = levels?.frames() ?? 0;
	if (strength !== "off" && !levels) {
		const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
		if (!asset?.file) throw new Error("The video's media file is not available");
		const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
		const length = seconds(element.duration);
		const sourceAt = (clipTime: number) =>
			seconds(element.trimStart) + getSourceTimeAtClipTime({ clipTime, retime: element.retime, duration: length });
		const a = sourceAt(0);
		const b = sourceAt(Math.max(0, length - 1 / TICKS_PER_SECOND));
		const measured = await measureLevels({
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
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId,
					patch: {
						params: {
							...current.params,
							[DEFLICKER_STRENGTH_PARAM]: strength,
							...(text !== null && { [DEFLICKER_LEVELS_PARAM]: text }),
						},
					} as Partial<TimelineElement>,
				},
			],
		}),
	});
	return { strength, frames, measured: text !== null };
}

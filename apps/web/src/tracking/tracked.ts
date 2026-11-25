/**
 * Tracked corner pin and tracked masks (row 92), the same as the desktop:
 * the shared `tracking` crate follows the points (`trackPoints`) and works
 * out the corner keys (`cornerKeys`) and the mask's stored motion
 * (`maskTrackEncode`, played back by both renderers with `maskTrackOffset`).
 */

import { cornerKeys, maskTrackEncode } from "opencut-wasm";
import type { ScalarChannel } from "@/animation/types";
import { BatchCommand } from "@/commands";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import type { EditorCore } from "@/core";
import { CORNER_PIN_ENABLED, CORNER_PIN_KEYS, PIN_CORNERS, boxToCanvas, canvasToBox, cornerPinCorners } from "@/corner-pin/pin";
import { getElementBounds } from "@/preview/element-bounds";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { mediaTime, roundMediaTime, TICKS_PER_SECOND } from "@/wasm";
import { KEY_TOLERANCE, TRACK_FPS, type TrackRef, trackPoints } from "./follow";

/** The param a tracked mask keeps its motion in (`tracking::MASK_TRACK_PARAM`). */
export const MASK_TRACK_PARAM = "track";

function find(editor: EditorCore, { trackId, elementId }: TrackRef): { track: TimelineTrack; element: TimelineElement } {
	const tracks: SceneTracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!track || !element) throw new Error("A clip to track is no longer on the timeline");
	return { track, element };
}

export interface TrackedResult {
	frames: number;
	lost: number;
	keys: number;
}

/**
 * Pins `pinned` to a surface in `video`: its corner pin's four corners at
 * the playhead (where the user placed them over a screen or sign) are
 * tracked through the video until either clip ends, and the pin gets keys
 * that keep each corner on its point. One undo step.
 */
export async function trackCornerPin({
	editor,
	video,
	pinned,
	onProgress,
}: {
	editor: EditorCore;
	video: TrackRef;
	pinned: TrackRef;
	onProgress?: (fraction: number) => void;
}): Promise<TrackedResult> {
	const source = find(editor, video);
	const target = find(editor, pinned);
	if (source.element.type !== "video") throw new Error("Pick a video clip to track");
	if (!["video", "image", "sticker"].includes(target.element.type) || target.element.id === source.element.id) {
		throw new Error("The pinned clip must be another video, picture or sticker");
	}
	if (target.track.locked) throw new Error("The pinned clip is on a locked track");
	const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
	const from = seconds(editor.playback.getCurrentTime());
	const pinnedStart = seconds(target.element.startTime);
	const to = Math.min(seconds(source.element.startTime + source.element.duration), pinnedStart + seconds(target.element.duration));
	if (from < seconds(source.element.startTime) || from < pinnedStart || from >= to) {
		throw new Error("Put the playhead where both the video and the pinned clip are");
	}
	const canvas = editor.project.getActive().settings.canvasSize;
	const mediaAsset =
		target.element.type === "video" || target.element.type === "image"
			? editor.media.getAssets().find((asset) => asset.id === (target.element as { mediaId: string }).mediaId)
			: undefined;
	const boundsAt = (localTicks: number) => {
		const bounds = getElementBounds({ element: target.element, canvasSize: canvas, mediaAsset, localTime: localTicks });
		if (!bounds) throw new Error("The pinned clip has no size to pin");
		return bounds;
	};
	const params = ("params" in target.element ? target.element.params : {}) as Record<string, unknown>;
	const startLocal = roundMediaTime({ time: (from - pinnedStart) * TICKS_PER_SECOND });
	// The pin's corners now, on the canvas (pixels from its centre).
	const startBox = boundsAt(startLocal);
	const corners = cornerPinCorners((key) => params[key]).map((corner) => {
		const [x, y] = boxToCanvas(startBox, corner);
		return [x - canvas.width / 2, y - canvas.height / 2] as [number, number];
	});
	const tracked = await trackPoints({
		editor,
		video: source.element,
		from,
		to,
		starts: (clip) => {
			const points = corners.map(([x, y]) => [0.5 + (x - clip.positionX) / clip.drawnWidth, 0.5 + (y - clip.positionY) / clip.drawnHeight] as [number, number]);
			if (points.some(([x, y]) => !(x > 0 && x < 1 && y > 0 && y < 1))) throw new Error("Every corner of the pin must sit on the video");
			return points;
		},
		onProgress,
	});
	const { placement } = tracked;
	const flat = cornerKeys(
		new Float64Array(tracked.times),
		new Float64Array(tracked.xs.flat()),
		new Float64Array(tracked.ys.flat()),
		placement.positionX,
		placement.positionY,
		placement.drawnWidth,
		placement.drawnHeight,
		new Float64Array(corners.flat()),
		KEY_TOLERANCE,
	);
	const duration = target.element.duration;
	const channels: ScalarChannel["keys"][] = CORNER_PIN_KEYS.map(() => []);
	const seen = new Set<number>();
	for (let index = 0; index + 8 < flat.length; index += 9) {
		const local = roundMediaTime({ time: Math.min(duration, Math.max(0, (flat[index] - pinnedStart) * TICKS_PER_SECOND)) });
		if (seen.has(local)) continue;
		seen.add(local);
		const box = boundsAt(local);
		PIN_CORNERS.forEach((_, corner) => {
			const [bx, by] = canvasToBox(box, [flat[index + 1 + corner * 2] + canvas.width / 2, flat[index + 2 + corner * 2] + canvas.height / 2]);
			channels[corner * 2].push({ id: generateUUID(), time: local, value: Math.round(bx * 10000) / 100, segmentToNext: "linear", tangentMode: "auto" });
			channels[corner * 2 + 1].push({ id: generateUUID(), time: local, value: Math.round(by * 10000) / 100, segmentToNext: "linear", tangentMode: "auto" });
		});
	}
	if (channels[0].length === 0) throw new Error("Nothing was tracked");
	editor.command.execute({
		command: new BatchCommand([
			new UpdateElementsCommand({
				updates: [{ trackId: pinned.trackId, elementId: pinned.elementId, patch: { params: { ...params, [CORNER_PIN_ENABLED]: true } } as Partial<TimelineElement> }],
			}),
			...CORNER_PIN_KEYS.map(
				(key, index) =>
					new SetChannelCommand({ trackId: pinned.trackId, elementId: pinned.elementId, propertyPath: key, channel: { keys: channels[index] } }),
			),
		]),
	});
	return { frames: tracked.times.length, lost: tracked.lost, keys: channels[0].length };
}

/**
 * Makes a video's mask follow motion: its centre at the playhead is
 * tracked through the video to the clip's end and the motion is stored on
 * the mask (`track`), which both editors play back. One undo step.
 */
export async function trackMask({
	editor,
	video,
	onProgress,
}: {
	editor: EditorCore;
	video: TrackRef;
	onProgress?: (fraction: number) => void;
}): Promise<TrackedResult> {
	const { track, element } = find(editor, video);
	if (element.type !== "video") throw new Error("Only a video's mask can follow motion");
	const mask = element.masks?.[0];
	if (!mask) throw new Error("Add a mask to the video first");
	if (track.locked) throw new Error("The video is on a locked track");
	const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
	const from = seconds(editor.playback.getCurrentTime());
	const start = seconds(element.startTime);
	const to = start + seconds(element.duration);
	if (from < start || from >= to) throw new Error("Put the playhead on the video clip");
	const maskParams = mask.params as unknown as Record<string, unknown>;
	const centerX = typeof maskParams.centerX === "number" ? maskParams.centerX : 0;
	const centerY = typeof maskParams.centerY === "number" ? maskParams.centerY : 0;
	const tracked = await trackPoints({
		editor,
		video: element,
		from,
		to,
		starts: () => {
			const point: [number, number] = [0.5 + centerX, 0.5 + centerY];
			if (!(point[0] > 0 && point[0] < 1 && point[1] > 0 && point[1] < 1)) throw new Error("The mask's centre must sit on the video");
			return [point];
		},
		onProgress,
	});
	const encoded = maskTrackEncode(
		new Float64Array(tracked.times.map((time) => time - start)),
		new Float64Array(tracked.xs[0]),
		new Float64Array(tracked.ys[0]),
		TRACK_FPS,
	);
	setMaskTrack({ editor, video, track: encoded });
	return { frames: tracked.times.length, lost: tracked.lost, keys: 0 };
}

/** Stores (or with "" clears) a video's mask track. One undo step. */
export function setMaskTrack({ editor, video, track }: { editor: EditorCore; video: TrackRef; track: string }): void {
	const { element } = find(editor, video);
	if (element.type !== "video" || !element.masks?.[0]) throw new Error("The video has no mask");
	const [mask, ...rest] = element.masks;
	const updated = { ...mask, params: { ...mask.params, [MASK_TRACK_PARAM]: track } } as unknown as typeof mask;
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [{ trackId: video.trackId, elementId: video.elementId, patch: { masks: [updated, ...rest] } as Partial<TimelineElement> }],
		}),
	});
}

export { mediaTime };

/**
 * Motion tracking in the web editor: the point under a follower (a sticker,
 * text, picture or clip placed over something in a video at the playhead)
 * is tracked through the video from there with the shared `tracking` crate
 * (WASM: `PointTracker`, `followKeys`), and the follower gets position keys
 * that keep it on that point, in one undo step. The desktop does the same
 * (`apps/desktop/src/tracking.rs`).
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { followKeys, PointTracker } from "opencut-wasm";
import type { ScalarChannel } from "@/animation/types";
import { BatchCommand } from "@/commands";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import type { EditorCore } from "@/core";
import { getClipTimeAtSourceTime, getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { roundMediaTime, TICKS_PER_SECOND } from "@/wasm";

/** Frames are tracked at this width. */
export const TRACK_WIDTH = 320;
/** At most this many frames a second are looked at. */
export const TRACK_FPS = 30;
/** The tracked box, as a share of the frame's shorter side. */
export const TRACK_BOX = 0.1;
/** Keys are kept within this many canvas pixels of the tracked path. */
export const KEY_TOLERANCE = 1;

export interface TrackRef {
	trackId: string;
	elementId: string;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

const FOLLOWERS = new Set(["video", "image", "sticker", "text", "graphic"]);

export interface FollowResult {
	/** Timeline seconds tracked. */
	from: number;
	to: number;
	frames: number;
	/** Frames where the point was not found (the follower held still). */
	lost: number;
	keys: number;
}

/**
 * Makes `follower` follow the point of `video` under its centre at timeline
 * time `at` (default: the playhead), from there to where either clip ends.
 */
export async function followMotion({
	editor,
	video,
	follower,
	at,
	onProgress,
}: {
	editor: EditorCore;
	video: TrackRef;
	follower: TrackRef;
	at?: number;
	onProgress?: (fraction: number) => void;
}): Promise<FollowResult> {
	const tracks = allTracks(editor.scenes.getActiveScene().tracks);
	const find = ({ trackId, elementId }: TrackRef) => {
		const track = tracks.find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!track || !element) throw new Error("A clip to track is no longer on the timeline");
		return { track, element };
	};
	const source = find(video);
	const target = find(follower);
	if (source.element.type !== "video") throw new Error("Pick a video clip to track");
	if (!FOLLOWERS.has(target.element.type) || target.element.id === source.element.id) {
		throw new Error("The follower must be another sticker, text, picture or clip");
	}
	if (target.track.locked) throw new Error("The follower is on a locked track");
	const videoElement = source.element;
	const asset = editor.media.getAssets().find((candidate) => candidate.id === videoElement.mediaId);
	if (!asset?.file) throw new Error("The video's media file is not available");

	const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
	const videoStart = seconds(videoElement.startTime);
	const videoLength = seconds(videoElement.duration);
	const followerStart = seconds(target.element.startTime);
	const followerEnd = followerStart + seconds(target.element.duration);
	const from = at ?? seconds(editor.playback.getCurrentTime());
	const to = Math.min(videoStart + videoLength, followerEnd);
	if (from < videoStart || from < followerStart || from >= to) {
		throw new Error("Put the playhead where both the video and the follower are");
	}

	const canvas = editor.project.getActive().settings.canvasSize;
	const params = videoElement.params as Record<string, unknown>;
	const number = (key: string, fallback: number) => (typeof params[key] === "number" ? (params[key] as number) : fallback);
	const followerParams = target.element.params as Record<string, unknown>;
	const pointX = typeof followerParams["transform.positionX"] === "number" ? (followerParams["transform.positionX"] as number) : 0;
	const pointY = typeof followerParams["transform.positionY"] === "number" ? (followerParams["transform.positionY"] as number) : 0;

	const trim = seconds(videoElement.trimStart);
	const sourceAt = (timeline: number) =>
		trim + getSourceTimeAtClipTime({ clipTime: timeline - videoStart, retime: videoElement.retime, duration: videoLength });
	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	const times: number[] = [];
	const xs: number[] = [];
	const ys: number[] = [];
	let lost = 0;
	let placement: { positionX: number; positionY: number; drawnWidth: number; drawnHeight: number };
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track || !(await track.canDecode())) throw new Error("The video cannot be decoded in this browser");
		const sourceWidth = track.displayWidth || asset.width || 1920;
		const sourceHeight = track.displayHeight || asset.height || 1080;
		const contain = Math.min(canvas.width / sourceWidth, canvas.height / sourceHeight);
		placement = {
			positionX: number("transform.positionX", 0),
			positionY: number("transform.positionY", 0),
			drawnWidth: sourceWidth * contain * Math.abs(number("transform.scaleX", 1)),
			drawnHeight: sourceHeight * contain * Math.abs(number("transform.scaleY", 1)),
		};
		const startX = 0.5 + (pointX - placement.positionX) / placement.drawnWidth;
		const startY = 0.5 + (pointY - placement.positionY) / placement.drawnHeight;
		if (!(startX > 0 && startX < 1 && startY > 0 && startY < 1)) {
			throw new Error("Place the follower over the thing in the video it should follow");
		}
		const width = TRACK_WIDTH;
		const height = Math.max(2, Math.round((TRACK_WIDTH * sourceHeight) / sourceWidth / 2) * 2);
		const box = Math.max(16, TRACK_BOX * Math.min(width, height));
		const sink = new CanvasSink(track, { width, height, fit: "fill" });
		const sourceFrom = Math.min(sourceAt(from), sourceAt(to));
		const sourceTo = Math.max(sourceAt(from), sourceAt(to));
		let tracker: PointTracker | null = null;
		let next = sourceFrom;
		for await (const wrapped of sink.canvases(sourceFrom, sourceTo)) {
			// A frame within a quarter of a step of the next sample time counts (timestamps are rounded).
			if (wrapped.timestamp + 0.25 / TRACK_FPS < next) continue;
			next = wrapped.timestamp + 1 / TRACK_FPS;
			const frame = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			const context = frame.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
			if (!context) continue;
			const rgba = new Uint8Array(context.getImageData(0, 0, width, height).data.buffer);
			const timeline =
				videoStart + getClipTimeAtSourceTime({ sourceTime: Math.max(0, wrapped.timestamp - trim), retime: videoElement.retime, duration: videoLength });
			let x: number;
			let y: number;
			if (!tracker) {
				try {
					tracker = new PointTracker(rgba, width, height, startX * width, startY * height, box, box);
				} catch (error) {
					throw new Error(error instanceof Error ? error.message : String(error));
				}
				[x, y] = tracker.position();
			} else {
				const step = tracker.step(rgba);
				[x, y] = step;
				if (step[3] === 1) lost++;
			}
			times.push(timeline - followerStart);
			xs.push(x / width);
			ys.push(y / height);
			onProgress?.(Math.min(1, (wrapped.timestamp - sourceFrom) / Math.max(1e-6, sourceTo - sourceFrom)));
		}
		tracker?.free();
	} finally {
		input.dispose();
	}
	if (times.length < 2) throw new Error("Not enough of the video to track from there");

	const flat = followKeys(
		new Float64Array(times),
		new Float64Array(xs),
		new Float64Array(ys),
		placement.positionX,
		placement.positionY,
		placement.drawnWidth,
		placement.drawnHeight,
		// The first frame's point is the follower's centre, give or take the edge clamp.
		pointX - (placement.positionX + (xs[0] - 0.5) * placement.drawnWidth),
		pointY - (placement.positionY + (ys[0] - 0.5) * placement.drawnHeight),
		KEY_TOLERANCE,
	);
	const duration = seconds(target.element.duration);
	const channel = (axis: 1 | 2): ScalarChannel => {
		const seen = new Set<number>();
		const keys: ScalarChannel["keys"] = [];
		for (let index = 0; index + 2 < flat.length; index += 3) {
			const time = roundMediaTime({ time: Math.min(duration, Math.max(0, flat[index])) * TICKS_PER_SECOND });
			if (seen.has(time)) continue;
			seen.add(time);
			keys.push({ id: generateUUID(), time, value: flat[index + axis], segmentToNext: "linear", tangentMode: "auto" });
		}
		return { keys };
	};
	const channelX = channel(1);
	const channelY = channel(2);
	editor.command.execute({
		command: new BatchCommand([
			new SetChannelCommand({ trackId: follower.trackId, elementId: follower.elementId, propertyPath: "transform.positionX", channel: channelX }),
			new SetChannelCommand({ trackId: follower.trackId, elementId: follower.elementId, propertyPath: "transform.positionY", channel: channelY }),
		]),
	});
	return { from, to: followerStart + times[times.length - 1], frames: times.length, lost, keys: channelX.keys.length };
}

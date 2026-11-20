/**
 * Face swap for a clip (`./client` does the server round trip): the clip's
 * source span is cut out (a video span is re-encoded as H.264 MP4 up to
 * 1080p with its sound; a picture is sent as it is), the face from the
 * photo goes onto the chosen face in it, and the clip then shows the
 * result, in one undo step. The result is a new media item marked as
 * generated (tool "faceswap"), so the credits list it and exports say the
 * video contains a swapped face. The clip remembers its original, so a new
 * swap starts from the original and "Restore original" puts it back.
 */

import {
	ALL_FORMATS,
	BlobSource,
	BufferTarget,
	Conversion,
	Input,
	Mp4OutputFormat,
	Output,
	QUALITY_HIGH,
	canEncodeVideo,
} from "mediabunny";
import { BatchCommand } from "@/commands/batch-command";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import { ToggleSourceAudioSeparationCommand } from "@/commands/timeline/element/toggle-source-audio-separation";
import type { EditorCore } from "@/core";
import { processMediaAssets } from "@/media/processing";
import type { MediaAsset } from "@/media/types";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import { isSourceAudioEnabled } from "@/timeline/audio-separation";
import type { ImageElement, TimelineElement, VideoElement } from "@/timeline/types";
import { mediaTimeFromSeconds, TICKS_PER_SECOND } from "@/wasm";
import { FACE_SWAP_MODEL, type FaceSwapQuality, type FaceSwapStage, runFaceSwap } from "./client";

/** The clip's original media, kept on the clip so swaps can be undone and redone from it. */
export const FACE_SWAP_SOURCE_PARAM = "faceswap.source";
/** The original trims, as JSON, for a video clip. */
export const FACE_SWAP_TRIM_PARAM = "faceswap.trim";
/** Swapping costs money by the second: longer spans are refused. */
export const MAX_SWAP_SECONDS = 60;
export const MAX_UPLOAD_BYTES = 90 * 1024 ** 2;
const MAX_FACE_EDGE = 1024;
const MAX_VIDEO_EDGE = 1920;

type SwapTarget = VideoElement | ImageElement;

interface OriginalTrim {
	trimStart: number;
	trimEnd: number;
	sourceDuration?: number;
}

function findClip({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): SwapTarget {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || (element.type !== "video" && element.type !== "image")) {
		throw new Error("Face swap needs a video or picture clip");
	}
	return element;
}

/** The clip's original media and trims (before any swap). */
export function originalOf({ element, assets }: { element: SwapTarget; assets: MediaAsset[] }): {
	asset: MediaAsset | undefined;
	trim: OriginalTrim;
	swapped: boolean;
} {
	const sourceId = String(element.params[FACE_SWAP_SOURCE_PARAM] ?? "");
	const swapped = sourceId !== "" && assets.some((asset) => asset.id === sourceId);
	const asset = assets.find((candidate) => candidate.id === (swapped ? sourceId : element.mediaId));
	let trim: OriginalTrim = { trimStart: element.trimStart, trimEnd: element.trimEnd, sourceDuration: element.sourceDuration };
	if (swapped && element.type === "video") {
		try {
			const stored = JSON.parse(String(element.params[FACE_SWAP_TRIM_PARAM] ?? "")) as OriginalTrim;
			if (typeof stored.trimStart === "number" && typeof stored.trimEnd === "number") trim = stored;
		} catch {
			// No stored trims: the clip's own are the best guess.
		}
	}
	return { asset, trim, swapped };
}

/** Seconds of the original the clip plays (its speed changes included). */
export function sourceSpanSeconds(element: VideoElement): number {
	const clipSeconds = element.duration / TICKS_PER_SECOND;
	return Math.max(
		getSourceTimeAtClipTime({ clipTime: 0, retime: element.retime, duration: clipSeconds }),
		getSourceTimeAtClipTime({ clipTime: clipSeconds, retime: element.retime, duration: clipSeconds }),
	);
}

/** Fits a frame inside `edge` pixels on its long side, in even numbers. */
export function fitWithin({ width, height, edge }: { width: number; height: number; edge: number }): { width: number; height: number } {
	const scale = Math.min(1, edge / Math.max(width, height));
	const even = (value: number) => Math.max(2, Math.round(value * scale)) & ~1;
	return { width: even(width), height: even(height) };
}

/** The face photo as a PNG, JPEG or WebP no larger than 1024 pixels a side. */
async function prepareFace(face: File): Promise<Blob> {
	const ok = /^image\/(png|jpeg|webp)$/.test(face.type);
	const bitmap = await createImageBitmap(face).catch(() => null);
	if (!bitmap) throw new Error("The face photo cannot be read; use a JPEG or PNG");
	try {
		if (ok && Math.max(bitmap.width, bitmap.height) <= MAX_FACE_EDGE && face.size <= 10 * 1024 ** 2) return face;
		const size = fitWithin({ width: bitmap.width, height: bitmap.height, edge: MAX_FACE_EDGE });
		const canvas = new OffscreenCanvas(size.width, size.height);
		const context = canvas.getContext("2d");
		if (!context) throw new Error("No 2D canvas");
		context.drawImage(bitmap, 0, 0, size.width, size.height);
		return await canvas.convertToBlob({ type: "image/jpeg", quality: 0.92 });
	} finally {
		bitmap.close();
	}
}

/** The picture clip's image as PNG, JPEG or WebP. */
async function preparePicture(asset: MediaAsset): Promise<Blob> {
	if (/^image\/(png|jpeg|webp)$/.test(asset.file.type)) return asset.file;
	const bitmap = await createImageBitmap(asset.file);
	try {
		const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
		const context = canvas.getContext("2d");
		if (!context) throw new Error("No 2D canvas");
		context.drawImage(bitmap, 0, 0);
		return await canvas.convertToBlob({ type: "image/png" });
	} finally {
		bitmap.close();
	}
}

/** The span `[from, to)` seconds of a video as an H.264 MP4 up to 1080p, with its sound. */
async function cutSpan({ file, from, to }: { file: File; from: number; to: number }): Promise<Blob> {
	const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track) throw new Error("The clip has no video");
		const size = fitWithin({
			width: track.displayWidth || 1920,
			height: track.displayHeight || 1080,
			edge: MAX_VIDEO_EDGE,
		});
		if (!(await canEncodeVideo("avc", size))) throw new Error("This browser cannot encode H.264 video, which the face swap needs");
		const target = new BufferTarget();
		const output = new Output({ format: new Mp4OutputFormat({ fastStart: "in-memory" }), target });
		const conversion = await Conversion.init({
			input,
			output,
			trim: { start: from, end: to },
			video: { width: size.width, height: size.height, fit: "contain", codec: "avc", bitrate: QUALITY_HIGH, forceTranscode: true },
			audio: { codec: "aac" },
		});
		if (!conversion.isValid) {
			throw new Error(
				`The clip could not be prepared: ${conversion.discardedTracks.map((discarded) => discarded.reason).join(", ") || "no usable tracks"}`,
			);
		}
		await conversion.execute();
		if (!target.buffer) throw new Error("The clip came out empty");
		return new Blob([target.buffer], { type: "video/mp4" });
	} finally {
		input.dispose();
	}
}

export interface FaceSwapResult {
	mediaId: string;
	name: string;
	seconds: number | null;
	/** The swapped video had no sound, so the clip's sound was moved to its own audio clip. */
	audioSeparated: boolean;
}

export async function swapFace({
	editor,
	trackId,
	elementId,
	face,
	faceIndex = 0,
	quality = "best",
	passcode,
	onStage,
	signal,
	run = runFaceSwap,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	face: File;
	faceIndex?: number;
	quality?: FaceSwapQuality;
	passcode: string;
	onStage?: (stage: FaceSwapStage | "preparing") => void;
	signal?: AbortSignal;
	run?: typeof runFaceSwap;
}): Promise<FaceSwapResult> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const element = findClip({ editor, trackId, elementId });
	const { asset, trim } = originalOf({ element, assets: editor.media.getAssets() });
	if (!asset?.file) throw new Error("The clip's media file is not available");

	onStage?.("preparing");
	let target: Blob;
	let seconds: number | null = null;
	if (element.type === "video") {
		seconds = sourceSpanSeconds(element);
		if (seconds > MAX_SWAP_SECONDS) {
			throw new Error(`Face swap takes clips up to ${MAX_SWAP_SECONDS} seconds; split this one first`);
		}
		const from = trim.trimStart / TICKS_PER_SECOND;
		target = await cutSpan({ file: asset.file, from, to: from + seconds });
	} else {
		target = await preparePicture(asset);
	}
	if (target.size > MAX_UPLOAD_BYTES) throw new Error("The clip is too large to send (over 90 MB); use a shorter span");

	const swapped = await run({
		kind: element.type,
		face: await prepareFace(face),
		target,
		faceIndex,
		quality,
		passcode,
		onStage,
		signal,
	});

	const base = asset.name.replace(/\.[^.]+$/, "");
	const isVideo = element.type === "video";
	const type = swapped.type && swapped.type !== "application/octet-stream" ? swapped.type : isVideo ? "video/mp4" : "image/png";
	const extension = type.includes("webm") ? "webm" : type.includes("jpeg") ? "jpg" : type.includes("webp") ? "webp" : isVideo ? "mp4" : "png";
	const [processed] = await processMediaAssets({
		files: [new File([swapped], `${base} (face swap).${extension}`, { type })],
	});
	if (!processed) throw new Error("The swapped clip could not be prepared");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool: "faceswap", from: asset.id, model: FACE_SWAP_MODEL } },
	});
	if (!saved) throw new Error("The swapped clip could not be stored");

	// Re-read the clip: it may have changed while the swap ran.
	const current = findClip({ editor, trackId, elementId });
	const params = {
		...current.params,
		[FACE_SWAP_SOURCE_PARAM]: asset.id,
		[FACE_SWAP_TRIM_PARAM]: isVideo ? JSON.stringify(trim) : "",
	};
	const patch: Partial<TimelineElement> = isVideo
		? ({
				mediaId: saved.id,
				trimStart: mediaTimeFromSeconds({ seconds: 0 }),
				trimEnd: mediaTimeFromSeconds({ seconds: 0 }),
				...(saved.duration != null ? { sourceDuration: mediaTimeFromSeconds({ seconds: saved.duration }) } : {}),
				params,
			} as Partial<TimelineElement>)
		: ({ mediaId: saved.id, params } as Partial<TimelineElement>);

	// A swapped video without sound would silence the clip: its original
	// sound moves to an audio clip of its own first.
	const loseSound =
		isVideo && asset.hasAudio !== false && saved.hasAudio === false && isSourceAudioEnabled({ element: current as VideoElement });
	const update = new UpdateElementsCommand({ updates: [{ trackId, elementId, patch }] });
	editor.command.execute({
		command: loseSound
			? new BatchCommand([new ToggleSourceAudioSeparationCommand({ trackId, elementId }), update])
			: update,
	});
	return { mediaId: saved.id, name: saved.name, seconds, audioSeparated: loseSound };
}

/** Shows the clip's original again (the swapped media stays in the project). */
export function restoreOriginal({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): boolean {
	const element = findClip({ editor, trackId, elementId });
	const { asset, trim, swapped } = originalOf({ element, assets: editor.media.getAssets() });
	if (!swapped || !asset) return false;
	const params = { ...element.params, [FACE_SWAP_SOURCE_PARAM]: "", [FACE_SWAP_TRIM_PARAM]: "" };
	const patch =
		element.type === "video"
			? { mediaId: asset.id, trimStart: trim.trimStart, trimEnd: trim.trimEnd, sourceDuration: trim.sourceDuration, params }
			: { mediaId: asset.id, params };
	editor.command.execute({
		command: new UpdateElementsCommand({ updates: [{ trackId, elementId, patch: patch as Partial<TimelineElement> }] }),
	});
	return true;
}


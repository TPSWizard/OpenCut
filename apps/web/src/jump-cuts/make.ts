/**
 * Smooth jump cuts on the web (row 106): for each planned bridge the frame
 * before it and the frame after it are decoded from the clip's file at full
 * size (mediabunny), the shared `jumpcut` crate (WASM) makes the in-between
 * frames, and they are encoded (H.264, else VP9; no sound) into a short
 * video added to the project. The bridges then go on the timeline in one
 * undo step. The desktop editor's `smooth_jump_cuts` does the same.
 */

import { ALL_FORMATS, BlobSource, BufferTarget, CanvasSink, CanvasSource, Input, Mp4OutputFormat, Output, QUALITY_VERY_HIGH, WebMOutputFormat, canEncodeVideo } from "mediabunny";
import { jumpCutFrame, jumpCutMotion } from "opencut-wasm";
import type { EditorCore } from "@/core";
import { frameRateToFloat } from "@/fps/utils";
import { processMediaAssets } from "@/media/processing";
import { mediaTimeToSeconds } from "@/wasm";
import {
	InsertJumpCutBridgesCommand,
	JUMP_CUT_DEFAULT_FRAMES,
	type JumpCut,
	type JumpCutBridgeJob,
	buildBridgeElement,
	planJumpCutBridge,
	projectJumpCuts,
} from "./cuts";

export interface SmoothJumpCutsResult {
	/** Cuts bridged. */
	smoothed: number;
	/** Cuts whose clips are too short for even two frames. */
	tooShort: number;
	bridges: { trackId: string; elementId: string; assetId: string; cutAt: number; frames: number; motion: number }[];
}

/** The jump cuts on the active scene. */
export function currentJumpCuts(editor: EditorCore): JumpCut[] {
	const project = editor.project.getActive();
	if (!project) return [];
	return projectJumpCuts({ tracks: editor.scenes.getActiveScene().tracks, fps: frameRateToFloat(project.settings.fps) });
}

type Picture = { data: Uint8Array; width: number; height: number };

async function framePair(file: File, from: number, to: number): Promise<[Picture, Picture]> {
	const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track || !(await track.canDecode())) throw new Error("The clip's video cannot be decoded in this browser");
		const sink = new CanvasSink(track, { poolSize: 0 });
		const first = await track.getFirstTimestamp();
		const grab = async (seconds: number): Promise<Picture> => {
			const wrapped = (await sink.getCanvas(Math.max(seconds, first))) ?? (await sink.getCanvas(first));
			if (!wrapped) throw new Error("A frame beside the cut could not be decoded");
			const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			const copy = new OffscreenCanvas(canvas.width, canvas.height);
			const context = copy.getContext("2d");
			if (!context) throw new Error("No 2D canvas");
			context.drawImage(canvas, 0, 0);
			const image = context.getImageData(0, 0, canvas.width, canvas.height);
			return { data: new Uint8Array(image.data.buffer), width: canvas.width, height: canvas.height };
		};
		return [await grab(from), await grab(to)];
	} finally {
		input.dispose();
	}
}

/** Makes one bridge's frames and encodes them; returns the file and the median motion in pixels. */
async function makeBridgeFile({ file, job, fps, name }: { file: File; job: JumpCutBridgeJob; fps: number; name: string }): Promise<{ file: File; motion: number }> {
	const [p, q] = await framePair(file, job.fromSource, job.toSource);
	if (p.width !== q.width || p.height !== q.height) throw new Error("The frames on either side of the cut differ in size");
	const { width, height } = p;
	const motion = jumpCutMotion(p.data, q.data, width, height);
	if (motion.length < 4) throw new Error("The frames could not be compared");
	const codec = (await canEncodeVideo("avc", { width, height })) ? "avc" : "vp9";
	if (codec === "vp9" && !(await canEncodeVideo("vp9", { width, height }))) {
		throw new Error("This browser cannot encode video, so the in-between frames cannot be stored");
	}
	const canvas = new OffscreenCanvas(width, height);
	const context = canvas.getContext("2d");
	if (!context) throw new Error("No 2D canvas");
	const target = new BufferTarget();
	const output = new Output({ format: codec === "avc" ? new Mp4OutputFormat({ fastStart: "in-memory" }) : new WebMOutputFormat(), target });
	const source = new CanvasSource(canvas, { codec, bitrate: QUALITY_VERY_HIGH, keyFrameInterval: 1 });
	output.addVideoTrack(source, { frameRate: fps });
	await output.start();
	for (const [index, alpha] of job.alphas.entries()) {
		const frame = jumpCutFrame(p.data, q.data, width, height, motion, alpha);
		const image = context.createImageData(width, height);
		image.data.set(frame);
		context.putImageData(image, 0, 0);
		await source.add(index / fps, 1 / fps);
	}
	await output.finalize();
	if (!target.buffer) throw new Error("The in-between frames came out empty");
	const extension = codec === "avc" ? "mp4" : "webm";
	return {
		file: new File([target.buffer], `${name}.${extension}`, { type: codec === "avc" ? "video/mp4" : "video/webm" }),
		motion: motion[3],
	};
}

/**
 * Bridges the jump cuts at `elementId` (default: every one on the
 * timeline) that have none yet with `frames` in-between frames each.
 */
export async function smoothJumpCuts({
	editor,
	elementId,
	frames = JUMP_CUT_DEFAULT_FRAMES,
	onProgress,
}: {
	editor: EditorCore;
	elementId?: string;
	frames?: number;
	onProgress?: (fraction: number) => void;
}): Promise<SmoothJumpCutsResult> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	if (!Number.isInteger(frames) || frames < 2 || frames > 30) throw new Error("A bridge is 2 to 30 frames");
	const fps = Math.max(1, frameRateToFloat(project.settings.fps));
	const tracks = editor.scenes.getActiveScene().tracks;
	const cuts = projectJumpCuts({ tracks, fps }).filter(
		(cut) => !cut.smoothed && (!elementId || cut.beforeId === elementId || cut.afterId === elementId),
	);
	if (cuts.length === 0) {
		throw new Error("No jump cuts to smooth: a jump cut is two clips from the same take back to back, with part of the take cut out between them");
	}
	const jobs = cuts.map((cut) => planJumpCutBridge({ tracks, cut, fps, frames })).filter((job): job is JumpCutBridgeJob => job !== null);
	const tooShort = cuts.length - jobs.length;
	if (jobs.length === 0) throw new Error("The clips at these cuts are too short to bridge");

	const made: { job: JumpCutBridgeJob; assetId: string; motion: number }[] = [];
	for (const [index, job] of jobs.entries()) {
		const asset = editor.media.getAssets().find((candidate) => candidate.id === job.mediaId);
		if (!asset?.file) throw new Error("The clip's media file is not available");
		const cutAt = mediaTimeToSeconds({ time: job.cut.time as Parameters<typeof mediaTimeToSeconds>[0]["time"] });
		const base = asset.name.replace(/\.[^.]+$/, "");
		const { file, motion } = await makeBridgeFile({ file: asset.file, job, fps, name: `${base} (smooth cut ${cutAt.toFixed(2)} s)` });
		const [processed] = await processMediaAssets({ files: [file] });
		if (!processed) throw new Error("The in-between frames could not be prepared");
		const saved = await editor.media.addMediaAsset({
			projectId: project.metadata.id,
			asset: { ...processed, origin: { kind: "generated", tool: "smooth-cut", from: asset.id } },
		});
		if (!saved) throw new Error("The in-between frames could not be stored");
		made.push({ job, assetId: saved.id, motion });
		onProgress?.((index + 1) / jobs.length);
	}

	const now = editor.scenes.getActiveScene().tracks;
	const bridges = made.flatMap((entry) => {
		const element = buildBridgeElement({ tracks: now, job: entry.job, mediaId: entry.assetId });
		return element ? [{ trackId: entry.job.cut.trackId, element, entry }] : [];
	});
	const command = new InsertJumpCutBridgesCommand(bridges.map(({ trackId, element }) => ({ trackId, element })));
	editor.command.execute({ command });
	const placed = command.getPlaced();
	return {
		smoothed: placed.length,
		tooShort,
		bridges: placed.flatMap((place) => {
			const entry = bridges.find((bridge) => bridge.element.id === place.elementId)?.entry;
			if (!entry) return [];
			return [
				{
					...place,
					assetId: entry.assetId,
					cutAt: mediaTimeToSeconds({ time: entry.job.cut.time as Parameters<typeof mediaTimeToSeconds>[0]["time"] }),
					frames: entry.job.frames,
					motion: entry.motion,
				},
			];
		}),
	};
}

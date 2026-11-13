/**
 * Background removal for a video clip: its source span is decoded small
 * (mediabunny), each frame is segmented for people (MediaPipe Selfie
 * Segmenter), the confidence is soft-thresholded and blurred over
 * neighbouring frames (`./matte`), and the matte is encoded as a small
 * greyscale video that is added to the project. The clip then gets a
 * `cutout` pointing at it; the renderers multiply it into the clip's mask.
 */

import {
	ALL_FORMATS,
	BlobSource,
	BufferTarget,
	CanvasSink,
	CanvasSource,
	Input,
	Mp4OutputFormat,
	Output,
	QUALITY_HIGH,
	WebMOutputFormat,
	canEncodeVideo,
} from "mediabunny";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { processMediaAssets } from "@/media/processing";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { Cutout, TimelineElement, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { TemporalSmoother, confidenceToLuma, keptShare, matteSize } from "./matte";

export const CUTOUT_DEFAULT_FPS = 15;

export interface CutoutResult {
	matteId: string;
	matteName: string;
	frames: number;
	seconds: number;
	/** Average share of each frame kept as the person. */
	keptShare: number;
}

function findVideo({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): VideoElement {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || element.type !== "video") throw new Error("Background removal needs a video clip");
	return element;
}

function setCutout({
	editor,
	trackId,
	elementId,
	cutout,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	cutout: Cutout | undefined;
}): void {
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [{ trackId, elementId, patch: { cutout } as Partial<TimelineElement> }],
		}),
	});
}

/** Segments the clip, stores the matte and gives the clip its cut-out (one undo step for the cut-out). */
export async function removeBackground({
	editor,
	trackId,
	elementId,
	sampleFps = CUTOUT_DEFAULT_FPS,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	sampleFps?: number;
	onProgress?: (fraction: number) => void;
}): Promise<CutoutResult> {
	const element = findVideo({ editor, trackId, elementId });
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");

	const clipSeconds = element.duration / TICKS_PER_SECOND;
	const span = Math.max(
		getSourceTimeAtClipTime({ clipTime: 0, retime: element.retime, duration: clipSeconds }),
		getSourceTimeAtClipTime({ clipTime: clipSeconds, retime: element.retime, duration: clipSeconds }),
	);
	const from = element.trimStart / TICKS_PER_SECOND;
	const to = from + span;

	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	let matteFile: File;
	let frames = 0;
	let keptTotal = 0;
	try {
		const videoTrack = await input.getPrimaryVideoTrack();
		if (!videoTrack || !(await videoTrack.canDecode())) {
			throw new Error("The clip's video cannot be decoded in this browser");
		}
		const { width, height } = matteSize({
			sourceWidth: videoTrack.displayWidth || asset.width || 1920,
			sourceHeight: videoTrack.displayHeight || asset.height || 1080,
		});
		const codec = (await canEncodeVideo("avc", { width, height })) ? "avc" : "vp9";
		if (codec === "vp9" && !(await canEncodeVideo("vp9", { width, height }))) {
			throw new Error("This browser cannot encode video, so the matte cannot be stored");
		}
		const segmenter = await (await import("./segment")).personSegmenter({ width, height });
		const canvas = new OffscreenCanvas(width, height);
		const context = canvas.getContext("2d");
		if (!context) throw new Error("No 2D canvas");
		const target = new BufferTarget();
		const output = new Output({
			format: codec === "avc" ? new Mp4OutputFormat({ fastStart: "in-memory" }) : new WebMOutputFormat(),
			target,
		});
		const source = new CanvasSource(canvas, { codec, bitrate: QUALITY_HIGH, keyFrameInterval: 1 });
		output.addVideoTrack(source, { frameRate: sampleFps });
		await output.start();

		const step = 1 / Math.max(1, sampleFps);
		const smoother = new TemporalSmoother();
		const times: number[] = [];
		let written = 0;
		const write = async (luma: Uint8Array) => {
			const image = context.createImageData(width, height);
			for (let i = 0; i < luma.length; i++) {
				image.data[i * 4] = luma[i];
				image.data[i * 4 + 1] = luma[i];
				image.data[i * 4 + 2] = luma[i];
				image.data[i * 4 + 3] = 255;
			}
			context.putImageData(image, 0, 0);
			const time = times[written];
			const next = times[written + 1] ?? time + step;
			await source.add(time, Math.max(1e-3, next - time));
			written += 1;
			keptTotal += keptShare({ luma });
		};

		const sink = new CanvasSink(videoTrack, { width, height, fit: "fill" });
		let nextSample = from;
		for await (const wrapped of sink.canvases(from, to)) {
			if (wrapped.timestamp + 1e-6 < nextSample) continue;
			nextSample = wrapped.timestamp + step;
			const confidence = segmenter.segment(wrapped.canvas as HTMLCanvasElement | OffscreenCanvas);
			const luma = confidence
				? confidenceToLuma({ confidence: confidence.values })
				: new Uint8Array(width * height);
			times.push(Math.max(0, wrapped.timestamp - from));
			frames += 1;
			const out = smoother.push(luma.length === width * height ? luma : new Uint8Array(width * height));
			if (out) await write(out);
			onProgress?.(Math.min(0.99, (wrapped.timestamp - from) / Math.max(1e-6, to - from)));
		}
		const last = smoother.flush();
		if (last) await write(last);
		if (frames === 0) throw new Error("No frames could be read from the clip");
		await output.finalize();
		if (!target.buffer) throw new Error("The matte came out empty");
		const base = asset.name.replace(/\.[^.]+$/, "");
		matteFile = new File([target.buffer], `${base} (cut-out matte).${codec === "avc" ? "mp4" : "webm"}`, {
			type: codec === "avc" ? "video/mp4" : "video/webm",
		});
	} finally {
		input.dispose();
	}

	const [processed] = await processMediaAssets({ files: [matteFile] });
	if (!processed) throw new Error("The matte could not be prepared");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool: "cutout", from: asset.id, model: "MediaPipe Selfie Segmenter" } },
	});
	if (!saved) throw new Error("The matte could not be stored");
	setCutout({ editor, trackId, elementId, cutout: { matteId: saved.id, offset: from, inverted: false } });
	onProgress?.(1);
	return { matteId: saved.id, matteName: saved.name, frames, seconds: span, keptShare: frames ? keptTotal / frames : 0 };
}

/** Keeps the background and hides the person instead (or back). */
export function setCutoutInverted({
	editor,
	trackId,
	elementId,
	inverted,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	inverted: boolean;
}): void {
	const element = findVideo({ editor, trackId, elementId });
	if (!element.cutout) throw new Error("The clip has no cut-out");
	setCutout({ editor, trackId, elementId, cutout: { ...element.cutout, inverted } });
}

/** Shows the whole clip again (the matte stays in the project's media). */
export function clearCutout({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): void {
	findVideo({ editor, trackId, elementId });
	setCutout({ editor, trackId, elementId, cutout: undefined });
}

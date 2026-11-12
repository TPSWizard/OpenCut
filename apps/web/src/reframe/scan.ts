/**
 * Auto-reframe of a video clip: its source span is decoded at a small size
 * (mediabunny), each sample is searched for faces (MediaPipe) and, as a
 * fallback, for movement and skin tone; shot changes come from the same
 * frames. The camera path from `./path` is written to the clip as a cover
 * scale and `transform.positionX` / `positionY` keys, in one undo step.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { BatchCommand } from "@/commands";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import type { EditorCore } from "@/core";
import type { ScalarChannel } from "@/animation/types";
import { getClipTimeAtSourceTime, getSourceTimeAtClipTime } from "@/retime/resolve";
import { SHOT_DEFAULTS, histogram, histogramDistance, pickCuts, type FrameScore } from "@/shots/detect";
import type { TimelineElement, VideoElement } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import {
	FACE_MIN_SCORE,
	isNoopGeometry,
	planReframe,
	reframeGeometry,
	SUBJECT_FRAME_WIDTH,
	shotSubjects,
	shrinkToRgb,
	subjectFromDetections,
	type ReframeCamera,
	type ReframeGeometry,
	type ReframePlan,
	type ReframeSample,
	type Subject,
} from "./path";

const SCAN_WIDTH = 320;
/** Frames shrink by this for the subject guess without faces (320 to 80). */
const SHRINK = SCAN_WIDTH / SUBJECT_FRAME_WIDTH;

export interface ReframeOptions {
	camera: ReframeCamera;
	/** Each shot gets its own framing, cut sharply. */
	resetAtShots: boolean;
	/** Look for faces (MediaPipe); otherwise movement and skin tone only. */
	faces: boolean;
	sampleFps: number;
}

export const REFRAME_DEFAULTS: ReframeOptions = { camera: "smooth", resetAtShots: true, faces: true, sampleFps: 5 };

export interface ReframeAnalysis {
	samples: ReframeSample[];
	/** Shot changes in clip seconds (used only with `resetAtShots`). */
	cuts: number[];
	geometry: ReframeGeometry;
	/** Samples whose subject came from a face. */
	faceSamples: number;
	/** Shots without any face, framed by movement and skin tone instead. */
	fallbackShots: number;
}

export interface ReframeResult extends ReframeAnalysis {
	plan: ReframePlan;
	keyframes: number;
}

function findVideo({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): VideoElement {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || element.type !== "video") {
		throw new Error("Auto-reframe needs a video clip");
	}
	return element;
}

/** The canvas shape and the clip's frame size, and whether there is anything to reframe. */
export function reframeGeometryFor({
	editor,
	sourceWidth,
	sourceHeight,
}: {
	editor: EditorCore;
	sourceWidth: number;
	sourceHeight: number;
}): ReframeGeometry {
	const canvas = editor.project.getActive()?.settings.canvasSize;
	if (!canvas) throw new Error("No project is open");
	return reframeGeometry({ sourceWidth, sourceHeight, canvasWidth: canvas.width, canvasHeight: canvas.height });
}

/** Samples the clip and finds the subject in each sample. */
export async function analyseReframe({
	editor,
	trackId,
	elementId,
	options = {},
	onProgress,
	detectFaces,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<ReframeOptions>;
	onProgress?: (fraction: number) => void;
	/** Injected for tests; defaults to MediaPipe. */
	detectFaces?: typeof import("./faces").detectFaces;
}): Promise<ReframeAnalysis> {
	const settings: ReframeOptions = { ...REFRAME_DEFAULTS, ...options };
	const element = findVideo({ editor, trackId, elementId });
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");
	const faceDetector = settings.faces ? (detectFaces ?? (await import("./faces")).detectFaces) : null;

	const clipSeconds = element.duration / TICKS_PER_SECOND;
	const trimSeconds = element.trimStart / TICKS_PER_SECOND;
	const span = Math.max(
		getSourceTimeAtClipTime({ clipTime: 0, retime: element.retime, duration: clipSeconds }),
		getSourceTimeAtClipTime({ clipTime: clipSeconds, retime: element.retime, duration: clipSeconds }),
	);
	const sourceFrom = trimSeconds;
	const sourceTo = trimSeconds + span;

	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	const raw: Array<{ time: number; face: Subject | null; small: Uint8Array }> = [];
	let smallSize = { width: 0, height: 0 };
	const scores: FrameScore[] = [];
	let geometry: ReframeGeometry;
	try {
		const videoTrack = await input.getPrimaryVideoTrack();
		if (!videoTrack || !(await videoTrack.canDecode())) {
			throw new Error("The clip's video cannot be decoded in this browser");
		}
		const sourceWidth = videoTrack.displayWidth || asset.width || 1920;
		const sourceHeight = videoTrack.displayHeight || asset.height || 1080;
		geometry = reframeGeometryFor({ editor, sourceWidth, sourceHeight });
		if (isNoopGeometry({ geometry })) {
			throw new Error("The canvas already shows the whole clip. Change the canvas shape first (9:16 for Shorts, Reels and TikTok).");
		}
		const width = SCAN_WIDTH;
		const height = Math.max(2, Math.round((SCAN_WIDTH * sourceHeight) / sourceWidth / 2) * 2);
		const sink = new CanvasSink(videoTrack, { width, height, fit: "fill" });
		const step = 1 / Math.max(1, settings.sampleFps);
		let previousHistogram: Float32Array | null = null;
		let nextSample = sourceFrom;
		for await (const wrapped of sink.canvases(sourceFrom, sourceTo)) {
			if (wrapped.timestamp + 1e-6 < nextSample) continue;
			nextSample = wrapped.timestamp + step;
			const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			const context = canvas.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
			if (!context) continue;
			const pixels = context.getImageData(0, 0, width, height).data;
			const time = getClipTimeAtSourceTime({
				sourceTime: Math.max(0, wrapped.timestamp - trimSeconds),
				retime: element.retime,
				duration: clipSeconds,
			});
			const face = faceDetector
				? subjectFromDetections({
						detections: await faceDetector({ image: canvas, width, height }),
						minScore: FACE_MIN_SCORE,
					})
				: null;
			const frameHistogram = histogram({ rgba: pixels });
			if (previousHistogram) {
				scores.push({ seconds: time, score: histogramDistance({ a: previousHistogram, b: frameHistogram }) });
			}
			const small = shrinkToRgb({ rgba: pixels, width, height, factor: SHRINK });
			smallSize = { width: small.width, height: small.height };
			raw.push({ time, face, small: small.rgb });
			previousHistogram = frameHistogram;
			onProgress?.(Math.min(1, (wrapped.timestamp - sourceFrom) / Math.max(1e-6, sourceTo - sourceFrom)));
		}
	} finally {
		input.dispose();
	}
	if (raw.length === 0) throw new Error("No frames could be read from the clip");

	// Reversed clips sample backwards in clip time.
	raw.sort((a, b) => a.time - b.time);
	scores.sort((a, b) => a.seconds - b.seconds);
	const allCuts = pickCuts({ scores, threshold: SHOT_DEFAULTS.threshold, minGapSeconds: SHOT_DEFAULTS.minGapSeconds })
		.map((cut) => cut.seconds)
		.filter((seconds) => seconds > 0 && seconds < clipSeconds);
	const cuts = settings.resetAtShots ? allCuts : [];

	// Within a shot that has faces, faces alone lead (gaps are filled); a
	// shot without any falls back to what stands out from its background,
	// worked out per shot either way.
	const bounds = [Number.NEGATIVE_INFINITY, ...allCuts, Number.POSITIVE_INFINITY];
	const samples: ReframeSample[] = [];
	let fallbackShots = 0;
	for (let shot = 0; shot < bounds.length - 1; shot++) {
		const inShot = raw.filter((s) => s.time >= bounds[shot] && s.time < bounds[shot + 1]);
		if (inShot.length === 0) continue;
		const hasFaces = inShot.some((s) => s.face !== null);
		if (!hasFaces) fallbackShots += 1;
		const guesses = hasFaces
			? null
			: shotSubjects({ frames: inShot.map((s) => s.small), width: smallSize.width, height: smallSize.height });
		inShot.forEach((s, i) => samples.push({ time: s.time, subject: hasFaces ? s.face : (guesses?.[i] ?? null) }));
	}
	return {
		samples,
		cuts,
		geometry,
		faceSamples: raw.filter((s) => s.face !== null).length,
		fallbackShots,
	};
}

function scalarChannel({ keys, duration }: { keys: Array<[number, number]>; duration: number }): ScalarChannel | undefined {
	if (keys.length === 0) return undefined;
	const seen = new Set<number>();
	const out: ScalarChannel["keys"] = [];
	for (const [seconds, value] of keys) {
		const time = roundMediaTime({ time: Math.min(duration, Math.max(0, seconds)) * TICKS_PER_SECOND });
		if (seen.has(time)) continue;
		seen.add(time);
		out.push({ id: generateUUID(), time, value, segmentToNext: "linear", tangentMode: "auto" });
	}
	return { keys: out };
}

/** Writes the plan to the clip: cover scale, start position, position keys (one undo step). */
export function applyReframePlan({
	editor,
	trackId,
	elementId,
	plan,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	plan: ReframePlan;
}): number {
	const element = findVideo({ editor, trackId, elementId });
	const duration = element.duration / TICKS_PER_SECOND;
	const channelX = scalarChannel({ keys: plan.keysX, duration });
	const channelY = scalarChannel({ keys: plan.keysY, duration });
	editor.command.execute({
		command: new BatchCommand([
			new UpdateElementsCommand({
				updates: [
					{
						trackId,
						elementId,
						patch: {
							params: {
								...element.params,
								"transform.scaleX": plan.scale,
								"transform.scaleY": plan.scale,
								"transform.positionX": plan.positionX,
								"transform.positionY": plan.positionY,
							},
						} as Partial<TimelineElement>,
					},
				],
			}),
			new SetChannelCommand({ trackId, elementId, propertyPath: "transform.scaleX", channel: undefined }),
			new SetChannelCommand({ trackId, elementId, propertyPath: "transform.scaleY", channel: undefined }),
			new SetChannelCommand({ trackId, elementId, propertyPath: "transform.positionX", channel: channelX }),
			new SetChannelCommand({ trackId, elementId, propertyPath: "transform.positionY", channel: channelY }),
		]),
	});
	return (channelX?.keys.length ?? 0) + (channelY?.keys.length ?? 0);
}

/** Analyses the clip and applies the reframe. */
export async function reframeClip({
	editor,
	trackId,
	elementId,
	options = {},
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<ReframeOptions>;
	onProgress?: (fraction: number) => void;
}): Promise<ReframeResult> {
	const settings: ReframeOptions = { ...REFRAME_DEFAULTS, ...options };
	const analysis = await analyseReframe({ editor, trackId, elementId, options: settings, onProgress });
	const plan = planReframe({
		samples: analysis.samples,
		cuts: analysis.cuts,
		geometry: analysis.geometry,
		camera: settings.camera,
	});
	const keyframes = applyReframePlan({ editor, trackId, elementId, plan });
	return { ...analysis, plan, keyframes };
}

/** Puts the clip back to fit the canvas: scale 1, centred, no position or scale keys. */
export function clearReframe({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): void {
	applyReframePlan({
		editor,
		trackId,
		elementId,
		plan: { scale: 1, positionX: 0, positionY: 0, keysX: [], keysY: [], shots: 0, samplesWithSubject: 0 },
	});
}

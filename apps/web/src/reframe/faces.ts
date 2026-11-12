/**
 * Face detection for auto-reframe: MediaPipe Tasks Vision's face detector
 * with the BlazeFace short-range model, both served from this site (the
 * wasm from the npm package, the model from `./models`; see ATTRIBUTION.md).
 * Loaded on first use only: the wasm is about 9.6 MB.
 */

import type { FaceDetector } from "@mediapipe/tasks-vision";
import wasmLoaderUrl from "@mediapipe-wasm/vision_wasm_internal.js?url";
import wasmBinaryUrl from "@mediapipe-wasm/vision_wasm_internal.wasm?url";
import modelUrl from "./models/blaze_face_short_range.tflite?url";
import { FACE_MIN_SCORE, type Detection } from "./path";

let detector: Promise<FaceDetector> | null = null;

function loadDetector(): Promise<FaceDetector> {
	detector ??= (async () => {
		const vision = await import("@mediapipe/tasks-vision");
		return vision.FaceDetector.createFromOptions(
			{ wasmLoaderPath: wasmLoaderUrl, wasmBinaryPath: wasmBinaryUrl },
			{
				baseOptions: { modelAssetPath: modelUrl, delegate: "CPU" },
				runningMode: "IMAGE",
				minDetectionConfidence: FACE_MIN_SCORE,
			},
		);
	})().catch((error) => {
		detector = null;
		throw error;
	});
	return detector;
}

/** The faces in a frame, as fractions of its size. */
export async function detectFaces({
	image,
	width,
	height,
}: {
	image: HTMLCanvasElement | OffscreenCanvas | ImageBitmap;
	width: number;
	height: number;
}): Promise<Detection[]> {
	const faces = await loadDetector();
	const result = faces.detect(image as HTMLCanvasElement);
	return result.detections.flatMap((found) => {
		const box = found.boundingBox;
		if (!box) return [];
		return [
			{
				x: box.originX / width,
				y: box.originY / height,
				width: box.width / width,
				height: box.height / height,
				score: found.categories[0]?.score ?? 0,
			},
		];
	});
}

/** Starts the download early (e.g. when the reframe section opens). */
export function warmUpFaceDetector(): void {
	void loadDetector().catch(() => {});
}

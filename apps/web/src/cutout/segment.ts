/**
 * Person segmentation for background removal: MediaPipe Tasks Vision's
 * image segmenter with the Selfie Segmenter models (landscape 144x256 for
 * wide frames, square 256x256 otherwise), served from this site and loaded
 * on first use. See ATTRIBUTION.md.
 */

import type { ImageSegmenter } from "@mediapipe/tasks-vision";
import wasmLoaderUrl from "@mediapipe-wasm/vision_wasm_internal.js?url";
import wasmBinaryUrl from "@mediapipe-wasm/vision_wasm_internal.wasm?url";
import landscapeModelUrl from "./models/selfie_segmenter_landscape.tflite?url";
import squareModelUrl from "./models/selfie_segmenter.tflite?url";

const segmenters = new Map<string, Promise<ImageSegmenter>>();

function loadSegmenter({ landscape }: { landscape: boolean }): Promise<ImageSegmenter> {
	const key = landscape ? "landscape" : "square";
	let segmenter = segmenters.get(key);
	if (!segmenter) {
		segmenter = (async () => {
			const vision = await import("@mediapipe/tasks-vision");
			return vision.ImageSegmenter.createFromOptions(
				{ wasmLoaderPath: wasmLoaderUrl, wasmBinaryPath: wasmBinaryUrl },
				{
					baseOptions: { modelAssetPath: landscape ? landscapeModelUrl : squareModelUrl, delegate: "CPU" },
					runningMode: "IMAGE",
					outputConfidenceMasks: true,
					outputCategoryMask: false,
				},
			);
		})().catch((error) => {
			segmenters.delete(key);
			throw error;
		});
		segmenters.set(key, segmenter);
	}
	return segmenter;
}

export interface PersonConfidence {
	/** 0 (background) to 1 (person) per pixel, row by row. */
	values: Float32Array;
	width: number;
	height: number;
}

/** A segmenter for frames of this shape: `segment` gives the person confidence of one frame. */
export async function personSegmenter({ width, height }: { width: number; height: number }): Promise<{
	segment: (image: HTMLCanvasElement | OffscreenCanvas | ImageBitmap) => PersonConfidence | null;
}> {
	const segmenter = await loadSegmenter({ landscape: width / Math.max(1, height) >= 1.2 });
	return {
		segment: (image) => {
			let result: PersonConfidence | null = null;
			segmenter.segment(image as HTMLCanvasElement, (output) => {
				const masks = output.confidenceMasks ?? [];
				// A two-class output is [background, person]; one channel is the person.
				const mask = masks.length >= 2 ? masks[1] : masks[0];
				if (!mask) return;
				result = { values: new Float32Array(mask.getAsFloat32Array()), width: mask.width, height: mask.height };
			});
			return result;
		},
	};
}

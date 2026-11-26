/**
 * Transparent export (row 97): the project's background left out and the
 * frames' alpha kept. Videos are VP9 with an alpha channel in WebM (the
 * library writes the alpha as a second VP9 stream, so no browser support
 * for alpha encoding is needed); stills and image sequences are PNGs with
 * alpha. ProRes 4444 is the desktop's (FFmpeg); no browser encodes it.
 *
 * The frames are read back from the shared compositor's texture
 * (`renderFrameRgba`), because the preview canvas it normally draws to is
 * opaque.
 */

import type { TBackground } from "@/project/types";

/** Nothing behind the clips: `buildScene` draws no colour or blur. */
export const TRANSPARENT_BACKGROUND: TBackground = { type: "color", color: "transparent" };

/** A transparent video is always WebM with VP9. */
export const TRANSPARENT_VIDEO = { format: "webm", codec: "vp9" } as const;

/** Kinds that can be transparent. */
export function kindTakesTransparency(kind: string): boolean {
	return kind === "video" || kind === "still" || kind === "frames";
}

/** Straight RGBA pixels as a PNG that keeps the alpha. */
export async function rgbaToPng({ rgba, width, height }: { rgba: Uint8Array; width: number; height: number }): Promise<Blob> {
	const canvas = new OffscreenCanvas(width, height);
	const context = canvas.getContext("2d");
	if (!context) throw new Error("No 2D canvas for the PNG");
	const pixels = new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength);
	context.putImageData(new ImageData(pixels, width, height), 0, 0);
	return canvas.convertToBlob({ type: "image/png" });
}

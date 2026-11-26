/**
 * More export kinds (row 96): an animated GIF through the shared `gifenc`
 * crate (WASM; the same palette, frames and bytes as the desktop), the
 * sound alone as WAV, AAC (.m4a) or Opus, and every frame as numbered PNGs
 * in a zip. The still PNG is the preview's snapshot.
 */

import { zipSync } from "fflate";
import {
	AudioBufferSource,
	BufferTarget,
	Mp4OutputFormat,
	OggOutputFormat,
	Output,
	Quality,
	WavOutputFormat,
} from "mediabunny";
import { GifEncoder, GifPalette, gifFrameDelays, gifSize } from "opencut-wasm";

export const EXPORT_KIND_VALUES = ["video", "gif", "audio", "still", "frames"] as const;
export type ExportKind = (typeof EXPORT_KIND_VALUES)[number];

export function isExportKind(value: unknown): value is ExportKind {
	return EXPORT_KIND_VALUES.some((kind) => kind === value);
}

export interface GifOptions {
	/** Width in pixels; the height keeps the canvas shape. */
	width: number;
	/** Frame rate, 1 to 50. */
	fps: number;
	/** Ordered dithering (smoother gradients, still areas stay still). */
	dither: boolean;
}

export const DEFAULT_GIF_OPTIONS: GifOptions = { width: 480, fps: 15, dither: true };
export const GIF_WIDTHS = [320, 480, 640] as const;
export const GIF_FRAME_RATES = [10, 15, 25] as const;
/** Most frames sampled for the palette (the desktop samples the same ones). */
export const GIF_PALETTE_SAMPLES = 40;
export const GIF_MAX_COLOURS = 255;

export type AudioExportFormat = "wav" | "aac" | "opus";

export const AUDIO_EXPORT_FORMATS: Record<AudioExportFormat, { label: string; extension: string; mimeType: string }> = {
	wav: { label: "WAV", extension: ".wav", mimeType: "audio/wav" },
	aac: { label: "AAC (.m4a)", extension: ".m4a", mimeType: "audio/mp4" },
	opus: { label: "Opus", extension: ".opus", mimeType: "audio/ogg" },
};

export function isAudioExportFormat(value: unknown): value is AudioExportFormat {
	return value === "wav" || value === "aac" || value === "opus";
}

export const GIF_MIME_TYPE = "image/gif";
export const FRAMES_MIME_TYPE = "application/zip";

/** The GIF frame size for a canvas: `[width, height]`, both even. */
export function gifFrameSize({ canvasWidth, canvasHeight, width }: { canvasWidth: number; canvasHeight: number; width: number }): [number, number] {
	const [w, h] = gifSize(canvasWidth, canvasHeight, Math.round(width));
	return [w, h];
}

/** How many GIF frames a timeline of `seconds` makes at `fps`. */
export function gifFrameCount({ seconds, fps }: { seconds: number; fps: number }): number {
	return Math.max(1, Math.ceil(seconds * fps));
}

/** Every `step`th frame goes into the palette. */
export function paletteStep(frameCount: number): number {
	return Math.max(1, Math.ceil(frameCount / GIF_PALETTE_SAMPLES));
}

/**
 * Encodes a GIF from frames `frameAt(index)` (RGBA at `width` × `height`):
 * a palette from every `sampleEvery`th frame, then every frame. Returns
 * null when cancelled. `onProgress` hears 0..1.
 */
export async function encodeGif({
	frameCount,
	fps,
	width,
	height,
	dither,
	colours = GIF_MAX_COLOURS,
	sampleEvery = paletteStep(frameCount),
	frameAt,
	onProgress,
	isCancelled,
}: {
	frameCount: number;
	fps: number;
	width: number;
	height: number;
	dither: boolean;
	colours?: number;
	sampleEvery?: number;
	frameAt: (index: number) => Promise<Uint8Array | Uint8ClampedArray>;
	onProgress?: (progress: number) => void;
	isCancelled?: () => boolean;
}): Promise<Uint8Array | null> {
	const samples = Math.ceil(frameCount / sampleEvery);
	const total = samples + frameCount;
	const palette = new GifPalette();
	let done = 0;
	try {
		for (let index = 0; index < frameCount; index += sampleEvery) {
			if (isCancelled?.()) return null;
			palette.addRgba(toBytes(await frameAt(index)));
			onProgress?.(++done / total);
		}
		const encoder = new GifEncoder(width, height, palette.colours(colours), dither);
		try {
			const delays = gifFrameDelays(frameCount, fps);
			for (let index = 0; index < frameCount; index++) {
				if (isCancelled?.()) return null;
				encoder.addRgba(toBytes(await frameAt(index)), delays[index]);
				onProgress?.(++done / total);
			}
			return encoder.finish();
		} finally {
			encoder.free();
		}
	} finally {
		palette.free();
	}
}

function toBytes(rgba: Uint8Array | Uint8ClampedArray): Uint8Array {
	return rgba instanceof Uint8Array ? rgba : new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength);
}

/** The mix written as `format`: 16-bit WAV, AAC in an MP4 audio file, or Opus in Ogg. */
export async function encodeAudioFile({ buffer, format }: { buffer: AudioBuffer; format: AudioExportFormat }): Promise<ArrayBuffer> {
	const output = new Output({
		format: format === "wav" ? new WavOutputFormat() : format === "aac" ? new Mp4OutputFormat({ fastStart: "in-memory" }) : new OggOutputFormat(),
		target: new BufferTarget(),
	});
	const source = new AudioBufferSource(
		format === "wav" ? { codec: "pcm-s16" } : { codec: format, bitrate: new Quality({ bitrate: 192_000 }) },
	);
	output.addAudioTrack(source);
	await output.start();
	await source.add(buffer);
	source.close();
	await output.finalize();
	const bytes = output.target.buffer;
	if (!bytes) throw new Error("The audio was not written");
	return bytes;
}

/** File name of frame `index` (from 1) in an image sequence, as the desktop names it. */
export function sequenceFileName(index: number): string {
	return `frame_${String(index).padStart(6, "0")}.png`;
}

/** Numbered PNGs in one zip (stored, not compressed again: PNGs are compressed already). */
export function zipFrames(frames: Uint8Array[]): Uint8Array {
	const entries: Record<string, [Uint8Array, { level: 0 }]> = {};
	frames.forEach((png, index) => {
		entries[sequenceFileName(index + 1)] = [png, { level: 0 }];
	});
	return zipSync(entries);
}

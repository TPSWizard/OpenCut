/**
 * Proxy media: a small H.264 (or VP9) copy of a video that the preview
 * decodes instead of the original; export always reads the original. The
 * size rule is the desktop's `crates/media/src/proxy.rs`: the longer side
 * at most 960 px, aspect kept, even sides, small sources unchanged.
 */

import {
	ALL_FORMATS,
	BlobSource,
	BufferTarget,
	Conversion,
	Input,
	Mp4OutputFormat,
	Output,
	QUALITY_MEDIUM,
	WebMOutputFormat,
} from "mediabunny";

export const PROXY_LONG_EDGE = 960;
/** Videos with more pixels than 1080p get a proxy on import. */
export const AUTO_PROXY_PIXELS = 1920 * 1080;

export type PreviewQuality = "auto" | "full" | "half" | "quarter";
export const PREVIEW_QUALITIES: { value: PreviewQuality; label: string }[] = [
	{ value: "auto", label: "Auto" },
	{ value: "full", label: "Full" },
	{ value: "half", label: "1/2" },
	{ value: "quarter", label: "1/4" },
];

export function isPreviewQuality(value: unknown): value is PreviewQuality {
	return PREVIEW_QUALITIES.some((quality) => quality.value === value);
}

const even = (value: number) => Math.max(2, Math.round(value)) & ~1;

export function proxySize({ width, height }: { width: number; height: number }): { width: number; height: number } {
	const w = Math.max(2, width);
	const h = Math.max(2, height);
	const long = Math.max(w, h);
	if (long <= PROXY_LONG_EDGE) return { width: even(w), height: even(h) };
	const scale = PROXY_LONG_EDGE / long;
	return { width: even(w * scale), height: even(h * scale) };
}

export function wantsProxy({ width, height }: { width?: number; height?: number }): boolean {
	return (width ?? 0) * (height ?? 0) > AUTO_PROXY_PIXELS;
}

/**
 * The size the preview decodes a source at, or `null` for its own size.
 * Auto fits the source inside the canvas (never larger than the source);
 * half and quarter scale that down; full decodes every source pixel.
 */
export function previewDecodeSize({
	sourceWidth,
	sourceHeight,
	canvas,
	quality,
}: {
	sourceWidth: number;
	sourceHeight: number;
	canvas: { width: number; height: number };
	quality: PreviewQuality;
}): { width: number; height: number } | null {
	if (quality === "full" || sourceWidth <= 0 || sourceHeight <= 0) return null;
	const fit = Math.min(1, canvas.width / sourceWidth, canvas.height / sourceHeight);
	const factor = quality === "half" ? 0.5 : quality === "quarter" ? 0.25 : 1;
	const scale = fit * factor;
	if (scale >= 1) return null;
	// Steps of 1/16 of the source keep a dragged canvas size from reopening decoders.
	const stepped = Math.max(1 / 16, Math.ceil(scale * 16) / 16);
	if (stepped >= 1) return null;
	return { width: Math.max(16, even(sourceWidth * stepped)), height: Math.max(16, even(sourceHeight * stepped)) };
}

export interface MadeProxy {
	file: File;
	width: number;
	height: number;
}

/**
 * Transcodes the video track of `file` to a proxy at {@link proxySize}:
 * H.264 in MP4 where the browser can encode it, else VP9 in WebM; audio
 * is dropped (the preview plays the original's audio), with a keyframe
 * every half second for quick seeks.
 */
export async function makeProxy({
	file,
	width,
	height,
	onProgress,
	signal,
}: {
	file: File;
	width: number;
	height: number;
	onProgress?: (progress: number) => void;
	signal?: AbortSignal;
}): Promise<MadeProxy> {
	const size = proxySize({ width, height });
	const reasons: string[] = [];
	for (const codec of ["avc", "vp9"] as const) {
		const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
		try {
			const target = new BufferTarget();
			const output = new Output({
				format: codec === "avc" ? new Mp4OutputFormat({ fastStart: "in-memory" }) : new WebMOutputFormat(),
				target,
			});
			const conversion = await Conversion.init({
				input,
				output,
				video: {
					width: size.width,
					height: size.height,
					fit: "contain",
					codec,
					bitrate: QUALITY_MEDIUM,
					keyFrameInterval: 0.5,
					forceTranscode: true,
				},
				audio: { discard: true },
			});
			if (!conversion.isValid || conversion.utilizedTracks.length === 0) {
				reasons.push(
					`${codec}: ${conversion.discardedTracks.map((track) => track.reason).join(", ") || "no video track"}`,
				);
				continue;
			}
			if (onProgress) conversion.onProgress = (progress) => onProgress(progress);
			const abort = () => void conversion.cancel();
			signal?.addEventListener("abort", abort, { once: true });
			try {
				await conversion.execute();
			} finally {
				signal?.removeEventListener("abort", abort);
			}
			if (!target.buffer) throw new Error("The proxy came out empty");
			const extension = codec === "avc" ? "mp4" : "webm";
			const base = file.name.replace(/\.[^.]+$/, "");
			return {
				file: new File([target.buffer], `${base}.proxy.${extension}`, {
					type: codec === "avc" ? "video/mp4" : "video/webm",
				}),
				width: size.width,
				height: size.height,
			};
		} finally {
			input.dispose();
		}
	}
	throw new Error(`This browser cannot make a proxy of the video (${reasons.join("; ")})`);
}

import type { FrameRate } from "opencut-wasm";
import type { PlatformEncodeSettings } from "@/platforms/catalogue";
import type { ExportVideoCodec } from "./codecs";
import type { MixLoudness, MixMasterReport } from "@/loudness/mix";
import { EXPORT_MIME_TYPES } from "./mime-types";
import type { AudioExportFormat, ExportKind, GifOptions } from "./kinds";

export const EXPORT_QUALITY_VALUES = [
	"low",
	"medium",
	"high",
	"very_high",
] as const;

export const EXPORT_FORMAT_VALUES = ["mp4", "webm"] as const;

export type ExportFormat = (typeof EXPORT_FORMAT_VALUES)[number];
export type ExportQuality = (typeof EXPORT_QUALITY_VALUES)[number];

export interface ExportOptions {
	/**
	 * What to write (row 96): a video (the default; `format` and the
	 * picture settings below), an animated GIF (`gif`), the sound alone
	 * (`audioFormat`, with `loudness`) or every frame as numbered PNGs in a
	 * zip. The still PNG is `editor.renderer.saveSnapshot`.
	 */
	kind?: Exclude<ExportKind, "still">;
	gif?: GifOptions;
	audioFormat?: AudioExportFormat;
	format: ExportFormat;
	quality: ExportQuality;
	fps?: FrameRate;
	includeAudio?: boolean;
	/**
	 * Encode for a platform upload: H.264 High profile at the platform
	 * bitrate with a half-second closed GOP, the moov atom first and
	 * 384 kbps audio. Only MP4 takes it; `quality` is ignored.
	 */
	platform?: PlatformEncodeSettings;
	/**
	 * Video codec; one the container does not take falls back to its
	 * default (MP4: H.264, WebM: VP9). Ignored with `platform`.
	 */
	videoCodec?: ExportVideoCodec;
	/** Target video bitrate in bits per second; replaces `quality` for the picture. */
	videoBitrate?: number;
	/** Ask the browser for a hardware encoder (faster; quality varies by GPU). */
	hardware?: boolean;
	/**
	 * Bring the whole mix to a loudness (`@/loudness/mix`: one gain and a
	 * true-peak limiter, the shared crate the desktop export uses).
	 */
	loudness?: MixLoudness;
}

export interface ExportResult {
	success: boolean;
	buffer?: ArrayBuffer;
	/** For kinds other than video: the file's type and extension (with the dot). */
	mimeType?: string;
	extension?: string;
	error?: string;
	cancelled?: boolean;
	/** What whole-mix loudness did, when `loudness` was asked for and there was sound. */
	mastered?: MixMasterReport;
}

export interface ExportState {
	isExporting: boolean;
	progress: number;
	result: ExportResult | null;
}

export function getExportMimeType({
	format,
}: {
	format: ExportFormat;
}): string {
	return EXPORT_MIME_TYPES[format];
}

export function getExportFileExtension({
	format,
}: {
	format: ExportFormat;
}): string {
	return `.${format}`;
}

export function downloadBuffer({
	buffer,
	filename,
	mimeType,
}: {
	buffer: ArrayBuffer;
	filename: string;
	mimeType: string;
}): void {
	const blob = new Blob([buffer], { type: mimeType });
	const url = URL.createObjectURL(blob);
	const downloadLink = document.createElement("a");
	downloadLink.href = url;
	downloadLink.download = filename;
	document.body.appendChild(downloadLink);
	downloadLink.click();
	document.body.removeChild(downloadLink);
	URL.revokeObjectURL(url);
}

import type { FrameRate } from "opencut-wasm";
import type { PlatformEncodeSettings } from "@/platforms/catalogue";
import type { ExportVideoCodec } from "./codecs";
import { EXPORT_MIME_TYPES } from "./mime-types";

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
}

export interface ExportResult {
	success: boolean;
	buffer?: ArrayBuffer;
	error?: string;
	cancelled?: boolean;
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

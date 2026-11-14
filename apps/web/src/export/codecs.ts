/**
 * Video codec choice for exports: which codecs each container takes, the
 * default, and reading a target bitrate typed in Mbps. The desktop has the
 * same rules (`crates/media/src/codecs.rs`); both test against
 * `fixtures/export-codecs/cases.json`.
 */

import { type Quality, type VideoCodec, canEncodeVideo } from "mediabunny";
import type { ExportFormat } from "./index";

export const EXPORT_VIDEO_CODEC_VALUES = ["h264", "hevc", "av1", "vp9"] as const;
export type ExportVideoCodec = (typeof EXPORT_VIDEO_CODEC_VALUES)[number];

export const CODECS_BY_FORMAT: Record<ExportFormat, readonly ExportVideoCodec[]> = {
	mp4: ["h264", "hevc", "av1"],
	webm: ["vp9", "av1"],
};

export const DEFAULT_CODEC: Record<ExportFormat, ExportVideoCodec> = {
	mp4: "h264",
	webm: "vp9",
};

export const CODEC_LABELS: Record<ExportVideoCodec, string> = {
	h264: "H.264",
	hevc: "HEVC (H.265)",
	av1: "AV1",
	vp9: "VP9",
};

export const MIN_BITRATE_MBPS = 0.2;
export const MAX_BITRATE_MBPS = 400;

export function isExportVideoCodec(value: unknown): value is ExportVideoCodec {
	return EXPORT_VIDEO_CODEC_VALUES.some((codec) => codec === value);
}

/** The codec to use: the one asked for when the container takes it, else the container's default. */
export function resolveVideoCodec({
	format,
	codec,
}: {
	format: ExportFormat;
	codec?: ExportVideoCodec | null;
}): ExportVideoCodec {
	return codec && CODECS_BY_FORMAT[format].includes(codec) ? codec : DEFAULT_CODEC[format];
}

/** Reads a codec name as people write it ("AVC", "h.265", "HEVC"). */
export function parseVideoCodec(text: string): ExportVideoCodec | null {
	const key = text.trim().toLowerCase().replace(/\./g, "");
	switch (key) {
		case "h264":
		case "avc":
			return "h264";
		case "h265":
		case "hevc":
			return "hevc";
		case "av1":
			return "av1";
		case "vp9":
			return "vp9";
		default:
			return null;
	}
}

/** Bits per second from a bitrate typed in Mbps, or null when it is not a number in range. */
export function parseBitrateMbps(text: string): number | null {
	const cleaned = text.trim().replace(",", ".");
	if (!/^\d+(\.\d+)?$/.test(cleaned)) return null;
	const mbps = Number(cleaned);
	if (!Number.isFinite(mbps) || mbps < MIN_BITRATE_MBPS || mbps > MAX_BITRATE_MBPS) return null;
	return Math.round(mbps * 1_000_000);
}

/** The codec name mediabunny and WebCodecs use. */
export function mediabunnyCodec(codec: ExportVideoCodec): VideoCodec {
	return codec === "h264" ? "avc" : codec;
}

/**
 * The codecs this browser can encode at this size, in the container's
 * order. Browsers differ: HEVC usually needs a hardware encoder, and AV1
 * encoding is missing from some.
 */
export async function encodableCodecs({
	format,
	width,
	height,
	bitrate,
	hardware = false,
	canEncode = canEncodeVideo,
}: {
	format: ExportFormat;
	width: number;
	height: number;
	bitrate?: number | Quality;
	hardware?: boolean;
	canEncode?: typeof canEncodeVideo;
}): Promise<ExportVideoCodec[]> {
	const codecs = CODECS_BY_FORMAT[format];
	const checks = await Promise.all(
		codecs.map((codec) =>
			canEncode(mediabunnyCodec(codec), {
				width,
				height,
				...(bitrate !== undefined ? { bitrate } : {}),
				hardwareAcceleration: hardware ? "prefer-hardware" : "no-preference",
			}).catch(() => false),
		),
	);
	return codecs.filter((_, index) => checks[index]);
}

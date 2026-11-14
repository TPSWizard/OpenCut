import EventEmitter from "eventemitter3";

import {
	Output,
	Mp4OutputFormat,
	WebMOutputFormat,
	BufferTarget,
	CanvasSource,
	AudioBufferSource,
	QUALITY_LOW,
	QUALITY_MEDIUM,
	QUALITY_HIGH,
	QUALITY_VERY_HIGH,
	Quality,
	canEncodeVideo,
} from "mediabunny";
import type { FrameRate } from "opencut-wasm";
import { mediaTimeToSeconds } from "opencut-wasm";
import { TICKS_PER_SECOND } from "@/wasm";
import { frameRateToFloat } from "@/fps/utils";
import type { RootNode } from "./nodes/root-node";
import type { ExportFormat, ExportQuality } from "@/export";
import type { PlatformEncodeSettings } from "@/platforms/catalogue";
import { CODEC_LABELS, type ExportVideoCodec, mediabunnyCodec, resolveVideoCodec } from "@/export/codecs";
import { CanvasRenderer } from "./canvas-renderer";

type ExportParams = {
	width: number;
	height: number;
	fps: FrameRate;
	format: ExportFormat;
	quality: ExportQuality;
	platform?: PlatformEncodeSettings;
	videoCodec?: ExportVideoCodec;
	videoBitrate?: number;
	hardware?: boolean;
	shouldIncludeAudio?: boolean;
	audioBuffer?: AudioBuffer;
};

const qualityMap = {
	low: QUALITY_LOW,
	medium: QUALITY_MEDIUM,
	high: QUALITY_HIGH,
	very_high: QUALITY_VERY_HIGH,
};

export type SceneExporterEvents = {
	progress: [progress: number];
	complete: [buffer: ArrayBuffer];
	error: [error: Error];
	cancelled: [];
};

export class SceneExporter extends EventEmitter<SceneExporterEvents> {
	private renderer: CanvasRenderer;
	private format: ExportFormat;
	private quality: ExportQuality;
	private platform: PlatformEncodeSettings | null;
	private videoCodec: ExportVideoCodec;
	private videoBitrate: number | null;
	private hardware: boolean;
	private shouldIncludeAudio: boolean;
	private audioBuffer?: AudioBuffer;

	private isCancelled = false;

	constructor({
		width,
		height,
		fps,
		format,
		quality,
		platform,
		videoCodec,
		videoBitrate,
		hardware,
		shouldIncludeAudio,
		audioBuffer,
	}: ExportParams) {
		super();
		this.renderer = new CanvasRenderer({
			width,
			height,
			fps,
		});

		this.format = format;
		this.quality = quality;
		this.platform = format === "mp4" ? (platform ?? null) : null;
		// Platform uploads are H.264 at the platform's bitrate.
		this.videoCodec = this.platform ? "h264" : resolveVideoCodec({ format, codec: videoCodec });
		this.videoBitrate = this.platform ? null : videoBitrate && videoBitrate > 0 ? videoBitrate : null;
		this.hardware = !this.platform && !!hardware;
		this.shouldIncludeAudio = shouldIncludeAudio ?? false;
		this.audioBuffer = audioBuffer;
	}

	cancel(): void {
		this.isCancelled = true;
	}

	async export({
		rootNode,
	}: {
		rootNode: RootNode;
	}): Promise<ArrayBuffer | null> {
		const fps = this.renderer.fps;
		const fpsFloat = frameRateToFloat(fps);
		const ticksPerFrame = Math.round(
			(TICKS_PER_SECOND * fps.denominator) / fps.numerator,
		);
		const frameCount = Math.floor(rootNode.duration / ticksPerFrame);

		const platform = this.platform;
		const outputFormat =
			this.format === "webm"
				? new WebMOutputFormat()
				: new Mp4OutputFormat(
						// Platforms want the moov atom first (fast start).
						platform ? { fastStart: "in-memory" } : {},
					);

		const output = new Output({
			format: outputFormat,
			target: new BufferTarget(),
		});

		const codec = mediabunnyCodec(this.videoCodec);
		const hardwareAcceleration = this.hardware ? ("prefer-hardware" as const) : ("no-preference" as const);
		const bitrate = this.videoBitrate ?? qualityMap[this.quality];
		if (
			!platform &&
			!(await canEncodeVideo(codec, {
				width: this.renderer.width,
				height: this.renderer.height,
				bitrate,
				hardwareAcceleration,
			}).catch(() => false))
		) {
			throw new Error(
				`This browser cannot encode ${CODEC_LABELS[this.videoCodec]} at ${this.renderer.width}×${this.renderer.height}. Choose another codec, or export from the desktop app.`,
			);
		}

		const videoSource = new CanvasSource(this.renderer.getOutputCanvas(), {
			codec,
			...(platform
				? {
						quality: new Quality({ bitrate: platform.videoBitrate }),
						keyFrameInterval: platform.keyFrameIntervalSeconds,
						...((await canEncodeWithCodecString({
							codec: platform.codec,
							width: this.renderer.width,
							height: this.renderer.height,
							bitrate: platform.videoBitrate,
							fps: fpsFloat,
						}))
							? { fullCodecString: platform.codec }
							: {}),
					}
				: { bitrate, hardwareAcceleration }),
		});

		output.addVideoTrack(videoSource, { frameRate: fpsFloat });

		let audioSource: AudioBufferSource | null = null;
		if (this.shouldIncludeAudio && this.audioBuffer) {
			let audioCodec: "aac" | "opus" = this.format === "webm" ? "opus" : "aac";

			if (audioCodec === "aac" && typeof AudioEncoder !== "undefined") {
				const { supported } = await AudioEncoder.isConfigSupported({
					codec: "mp4a.40.2",
					sampleRate: this.audioBuffer.sampleRate,
					numberOfChannels: this.audioBuffer.numberOfChannels,
					bitrate: 192000,
				});
				if (!supported) audioCodec = "opus";
			}

			audioSource = new AudioBufferSource({
				codec: audioCodec,
				bitrate: platform
					? new Quality({ bitrate: platform.audioBitrate })
					: qualityMap[this.quality],
			});
			output.addAudioTrack(audioSource);
		}

		await output.start();

		if (audioSource && this.audioBuffer) {
			await audioSource.add(this.audioBuffer);
			audioSource.close();
		}

		for (let i = 0; i < frameCount; i++) {
			if (this.isCancelled) {
				await output.cancel();
				this.emit("cancelled");
				return null;
			}

			const timeTicks = i * ticksPerFrame;
			const timeSeconds = mediaTimeToSeconds({ time: timeTicks });
			await this.renderer.render({ node: rootNode, time: timeTicks });
			await videoSource.add(timeSeconds, 1 / fpsFloat);

			this.emit("progress", i / frameCount);
		}

		if (this.isCancelled) {
			await output.cancel();
			this.emit("cancelled");
			return null;
		}

		videoSource.close();
		await output.finalize();
		this.emit("progress", 1);

		const buffer = output.target.buffer;
		if (!buffer) {
			this.emit("error", new Error("Failed to export video"));
			return null;
		}

		this.emit("complete", buffer);
		return buffer;
	}
}

/**
 * Whether this browser's encoder takes the exact codec string (profile and
 * level). When it does not, the library's own string is used instead, which
 * keeps the bitrate and GOP but may land on a different profile.
 */
async function canEncodeWithCodecString({
	codec,
	width,
	height,
	bitrate,
	fps,
}: {
	codec: string;
	width: number;
	height: number;
	bitrate: number;
	fps: number;
}): Promise<boolean> {
	if (typeof VideoEncoder === "undefined") return false;
	try {
		const { supported } = await VideoEncoder.isConfigSupported({
			codec,
			width,
			height,
			bitrate,
			framerate: fps,
		});
		return !!supported;
	} catch {
		return false;
	}
}

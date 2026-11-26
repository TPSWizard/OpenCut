import { FACE_SWAP_EXPORT_NOTE, usesFaceSwap } from "@/faceswap/disclosure";
import type { EditorCore } from "@/core";
import type { RootNode } from "@/services/renderer/nodes/root-node";
import type { ExportOptions, ExportResult } from "@/export";
import { CanvasRenderer } from "@/services/renderer/canvas-renderer";
import { SceneExporter } from "@/services/renderer/scene-exporter";
import { buildScene } from "@/services/renderer/scene-builder";
import { createTimelineAudioBuffer } from "@/media/audio";
import {
	AUDIO_EXPORT_FORMATS,
	DEFAULT_GIF_OPTIONS,
	FRAMES_MIME_TYPE,
	GIF_MIME_TYPE,
	encodeAudioFile,
	encodeGif,
	gifFrameCount,
	gifFrameSize,
	zipFrames,
} from "@/export/kinds";
import { TICKS_PER_SECOND } from "@/wasm";
import { frameRateToFloat } from "@/fps/utils";
import { formatTimecode } from "opencut-wasm";
import { downloadBlob } from "@/utils/browser";
import { TRANSPARENT_BACKGROUND, rgbaToPng } from "@/export/transparent";

type SnapshotResult =
	| { success: true; blob: Blob; filename: string }
	| { success: false; error: string };

export class RendererManager {
	private renderTree: RootNode | null = null;
	private _isDegraded = false;
	private listeners = new Set<() => void>();

	constructor(private editor: EditorCore) {}

	get isDegraded(): boolean {
		return this._isDegraded;
	}

	setDegraded(degraded: boolean): void {
		if (this._isDegraded === degraded) return;
		this._isDegraded = degraded;
		this.notify();
	}

	setRenderTree({ renderTree }: { renderTree: RootNode | null }): void {
		this.renderTree = renderTree;
		this.notify();
	}

	getRenderTree(): RootNode | null {
		return this.renderTree;
	}

	private redraws = 0;

	/**
	 * Asks the preview to draw its frame again: off-screen renders (cover
	 * pictures) share the compositor canvas the preview shows.
	 */
	requestRedraw(): void {
		this.redraws++;
		this.notify();
	}

	getRedrawCount(): number {
		return this.redraws;
	}

	async saveSnapshot({ transparent = false }: { transparent?: boolean } = {}): Promise<{ success: boolean; error?: string }> {
		const snapshot = transparent ? await this.createTransparentSnapshot() : await this.createSnapshot();
		if (!snapshot.success) {
			return snapshot;
		}

		downloadBlob({ blob: snapshot.blob, filename: snapshot.filename });
		return { success: true };
	}

	async copySnapshot(): Promise<{ success: boolean; error?: string }> {
		if (typeof ClipboardItem === "undefined" || !navigator.clipboard?.write) {
			return {
				success: false,
				error: "Clipboard image copy is not supported in this browser",
			};
		}

		const snapshot = await this.createSnapshot();
		if (!snapshot.success) {
			return snapshot;
		}

		try {
			await navigator.clipboard.write([
				new ClipboardItem({
					[snapshot.blob.type || "image/png"]: snapshot.blob,
				}),
			]);
			return { success: true };
		} catch (error) {
			console.error("Copy snapshot failed:", error);
			return {
				success: false,
				error: error instanceof Error ? error.message : "Unknown error",
			};
		}
	}

	/** The frame at the playhead as a PNG at the canvas size; with nothing behind the clips when `transparent` (row 97). */
	async renderStillPng({ transparent = false }: { transparent?: boolean } = {}): Promise<SnapshotResult> {
		return transparent ? this.createTransparentSnapshot() : this.createSnapshot();
	}

	/** The frame at the playhead without the background, as a PNG with alpha. */
	private async createTransparentSnapshot(): Promise<SnapshotResult> {
		try {
			const activeProject = this.editor.project.getActive();
			const duration = this.editor.timeline.getTotalDuration();
			if (!activeProject) return { success: false, error: "No project or scene to capture" };
			if (duration === 0) return { success: false, error: "Project is empty" };
			const { canvasSize, fps } = activeProject.settings;
			const renderTime = Math.min(this.editor.playback.getCurrentTime(), this.editor.timeline.getLastFrameTime());
			const scene = buildScene({
				tracks: this.editor.scenes.getActiveScene().tracks,
				mediaAssets: this.editor.media.getAssets(),
				duration,
				canvasSize,
				background: TRANSPARENT_BACKGROUND,
			});
			const renderer = new CanvasRenderer({ width: canvasSize.width, height: canvasSize.height, fps });
			const rgba = await renderer.renderRgba({ node: scene, time: renderTime });
			const blob = await rgbaToPng({ rgba, width: canvasSize.width, height: canvasSize.height });
			this.requestRedraw();
			const timecode = formatTimecode({ time: renderTime, rate: fps })!.replace(/:/g, "-");
			const safeName = activeProject.metadata.name.replace(/[<>:"/\\|?*]/g, "-").trim() || "snapshot";
			return { success: true, blob, filename: `${safeName}-${timecode}.png` };
		} catch (error) {
			console.error("Transparent snapshot failed:", error);
			return { success: false, error: error instanceof Error ? error.message : "Unknown error" };
		}
	}

	private async createSnapshot(): Promise<SnapshotResult> {
		try {
			const renderTree = this.getRenderTree();
			const activeProject = this.editor.project.getActive();

			if (!renderTree || !activeProject) {
				return { success: false, error: "No project or scene to capture" };
			}

			const duration = this.editor.timeline.getTotalDuration();
			if (duration === 0) {
				return { success: false, error: "Project is empty" };
			}

			const { canvasSize, fps } = activeProject.settings;
			const renderTime = Math.min(
				this.editor.playback.getCurrentTime(),
				this.editor.timeline.getLastFrameTime(),
			);

			const renderer = new CanvasRenderer({
				width: canvasSize.width,
				height: canvasSize.height,
				fps,
			});

			const tempCanvas = document.createElement("canvas");
			tempCanvas.width = canvasSize.width;
			tempCanvas.height = canvasSize.height;

			await renderer.renderToCanvas({
				node: renderTree,
				time: renderTime,
				targetCanvas: tempCanvas,
			});

			const blob = await new Promise<Blob | null>((resolve) => {
				tempCanvas.toBlob((result) => resolve(result), "image/png");
			});

			if (!blob) {
				return { success: false, error: "Failed to create image" };
			}

			const timecode = formatTimecode({ time: renderTime, rate: fps })!.replace(/:/g, "-");
			const safeName =
				activeProject.metadata.name.replace(/[<>:"/\\|?*]/g, "-").trim() ||
				"snapshot";
			const filename = `${safeName}-${timecode}.png`;

			return { success: true, blob, filename };
		} catch (error) {
			console.error("Snapshot capture failed:", error);
			return {
				success: false,
				error: error instanceof Error ? error.message : "Unknown error",
			};
		}
	}

	async exportProject({
		options,
		onProgress,
		onCancel,
	}: {
		options: ExportOptions;
		onProgress?: ({ progress }: { progress: number }) => void;
		onCancel?: () => boolean;
	}): Promise<ExportResult> {
		const { format, quality, fps, includeAudio, platform, videoCodec, videoBitrate, hardware, loudness } = options;
		let mastered: ExportResult["mastered"];
		if (options.kind && options.kind !== "video") {
			try {
				return await this.exportOtherKind({ options, onProgress, onCancel });
			} catch (error) {
				console.error("Export failed:", error);
				return { success: false, error: error instanceof Error ? error.message : "Unknown export error" };
			}
		}

		try {
			const tracks = this.editor.scenes.getActiveScene().tracks;
			const mediaAssets = this.editor.media.getAssets();
			const activeProject = this.editor.project.getActive();

			if (!activeProject) {
				return { success: false, error: "No active project" };
			}

			const duration = this.editor.timeline.getTotalDuration();
			if (duration === 0) {
				return { success: false, error: "Project is empty" };
			}

			const exportFps = fps ?? activeProject.settings.fps;
			const canvasSize = activeProject.settings.canvasSize;

			let audioBuffer: AudioBuffer | null = null;
			if (includeAudio) {
				onProgress?.({ progress: 0.05 });
				audioBuffer = await createTimelineAudioBuffer({
					tracks,
					mediaAssets,
					duration,
					loudness,
					onMastered: (report) => {
						mastered = report;
					},
				});
			}

			const transparent = !!options.transparent;
			const scene = buildScene({
				tracks,
				mediaAssets,
				duration,
				canvasSize,
				background: transparent ? TRANSPARENT_BACKGROUND : activeProject.settings.background,
			});

			// The project's cover, embedded in MP4s when it is set to be.
			let cover: Uint8Array | undefined;
			const coverSettings = activeProject.settings.cover;
			if (format === "mp4" && !transparent && coverSettings?.embed) {
				try {
					const { renderCoverPng } = await import("@/cover/render");
					const { blob } = await renderCoverPng({ editor: this.editor });
					cover = new Uint8Array(await blob.arrayBuffer());
				} catch (error) {
					console.error("Cover not embedded", error);
				}
			}

			const exporter = new SceneExporter({
				cover,
				width: canvasSize.width,
				height: canvasSize.height,
				fps: exportFps,
				format,
				quality,
				platform,
				videoCodec,
				videoBitrate,
				hardware,
				shouldIncludeAudio: !!includeAudio,
				audioBuffer: audioBuffer || undefined,
				transparent,
				disclosure: usesFaceSwap({
					assets: mediaAssets,
					mediaIds: [tracks.main, ...tracks.overlay].flatMap((track) =>
						track.elements.flatMap((element) =>
							"mediaId" in element && typeof element.mediaId === "string" ? [element.mediaId] : [],
						),
					),
				})
					? FACE_SWAP_EXPORT_NOTE
					: undefined,
			});

			exporter.on("progress", (progress) => {
				const adjustedProgress = includeAudio
					? 0.05 + progress * 0.95
					: progress;
				onProgress?.({ progress: adjustedProgress });
			});

			let cancelled = false;
			const checkCancel = () => {
				if (onCancel?.()) {
					cancelled = true;
					exporter.cancel();
				}
			};

			const cancelInterval = setInterval(checkCancel, 100);

			try {
				const buffer = await exporter.export({ rootNode: scene });
				clearInterval(cancelInterval);

				if (cancelled) {
					return { success: false, cancelled: true };
				}

				if (!buffer) {
					return { success: false, error: "Export failed to produce buffer" };
				}

				return {
					success: true,
					buffer,
					mastered,
				};
			} finally {
				clearInterval(cancelInterval);
			}
		} catch (error) {
			console.error("Export failed:", error);
			return {
				success: false,
				error: error instanceof Error ? error.message : "Unknown export error",
			};
		}
	}

	/** GIF, audio only or an image sequence (row 96). */
	private async exportOtherKind({
		options,
		onProgress,
		onCancel,
	}: {
		options: ExportOptions;
		onProgress?: ({ progress }: { progress: number }) => void;
		onCancel?: () => boolean;
	}): Promise<ExportResult> {
		const tracks = this.editor.scenes.getActiveScene().tracks;
		const mediaAssets = this.editor.media.getAssets();
		const activeProject = this.editor.project.getActive();
		if (!activeProject) return { success: false, error: "No active project" };
		const duration = this.editor.timeline.getTotalDuration();
		if (duration === 0) return { success: false, error: "Project is empty" };
		const seconds = duration / TICKS_PER_SECOND;
		const cancelled = () => onCancel?.() === true;

		if (options.kind === "audio") {
			const format = options.audioFormat ?? "aac";
			let mastered: ExportResult["mastered"];
			onProgress?.({ progress: 0.1 });
			const buffer = await createTimelineAudioBuffer({
				tracks,
				mediaAssets,
				duration,
				loudness: options.loudness,
				onMastered: (report) => {
					mastered = report;
				},
			});
			if (!buffer) return { success: false, error: "The timeline has no sound" };
			if (cancelled()) return { success: false, cancelled: true };
			onProgress?.({ progress: 0.6 });
			const bytes = await encodeAudioFile({ buffer, format });
			onProgress?.({ progress: 1 });
			const { extension, mimeType } = AUDIO_EXPORT_FORMATS[format];
			return { success: true, buffer: bytes, mastered, extension, mimeType };
		}

		const { canvasSize } = activeProject.settings;
		const transparent = options.kind === "frames" && !!options.transparent;
		const scene = buildScene({
			tracks,
			mediaAssets,
			duration,
			canvasSize,
			background: transparent ? TRANSPARENT_BACKGROUND : activeProject.settings.background,
		});
		const renderer = new CanvasRenderer({ width: canvasSize.width, height: canvasSize.height, fps: options.fps ?? activeProject.settings.fps });
		const output = renderer.getOutputCanvas();

		if (options.kind === "gif") {
			const gif = { ...DEFAULT_GIF_OPTIONS, ...options.gif };
			const fps = Math.min(50, Math.max(1, gif.fps));
			const [width, height] = gifFrameSize({ canvasWidth: canvasSize.width, canvasHeight: canvasSize.height, width: gif.width });
			const small = document.createElement("canvas");
			small.width = width;
			small.height = height;
			const context = small.getContext("2d", { willReadFrequently: true });
			if (!context) return { success: false, error: "No 2D canvas for the GIF frames" };
			context.imageSmoothingEnabled = true;
			context.imageSmoothingQuality = "high";
			const bytes = await encodeGif({
				frameCount: gifFrameCount({ seconds, fps }),
				fps,
				width,
				height,
				dither: gif.dither,
				frameAt: async (index) => {
					await renderer.render({ node: scene, time: Math.round((index * TICKS_PER_SECOND) / fps) });
					context.clearRect(0, 0, width, height);
					context.drawImage(output, 0, 0, width, height);
					return context.getImageData(0, 0, width, height).data;
				},
				onProgress: (progress) => onProgress?.({ progress }),
				isCancelled: cancelled,
			});
			if (!bytes) return { success: false, cancelled: true };
			return { success: true, buffer: copyToArrayBuffer(bytes), extension: ".gif", mimeType: GIF_MIME_TYPE };
		}

		// Every frame at the project's rate, as numbered PNGs in a zip.
		const fps = frameRateToFloat(options.fps ?? activeProject.settings.fps);
		const count = Math.max(1, Math.ceil(seconds * fps - 1e-9));
		const frames: Uint8Array[] = [];
		for (let index = 0; index < count; index++) {
			if (cancelled()) return { success: false, cancelled: true };
			const time = Math.round((index * TICKS_PER_SECOND) / fps);
			let blob: Blob | null;
			if (transparent) {
				const rgba = await renderer.renderRgba({ node: scene, time });
				blob = await rgbaToPng({ rgba, width: canvasSize.width, height: canvasSize.height });
			} else {
				await renderer.render({ node: scene, time });
				blob = await new Promise<Blob | null>((resolve) => output.toBlob(resolve, "image/png"));
			}
			if (!blob) return { success: false, error: "A frame could not be saved as PNG" };
			frames.push(new Uint8Array(await blob.arrayBuffer()));
			onProgress?.({ progress: ((index + 1) / count) * 0.95 });
		}
		const zip = zipFrames(frames);
		onProgress?.({ progress: 1 });
		return { success: true, buffer: copyToArrayBuffer(zip), extension: " frames.zip", mimeType: FRAMES_MIME_TYPE };
	}

	subscribe(listener: () => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	private notify(): void {
		this.listeners.forEach((fn) => {
			fn();
		});
	}
}

/** A standalone ArrayBuffer holding `bytes`. */
function copyToArrayBuffer(bytes: Uint8Array): ArrayBuffer {
	const copy = new Uint8Array(bytes.byteLength);
	copy.set(bytes);
	return copy.buffer;
}

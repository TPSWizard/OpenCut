import { drawVideoScope } from "opencut-wasm";
import { create } from "zustand";

/**
 * Video scopes of the preview: a waveform, RGB parade, vectorscope or
 * histogram drawn by the shared `scopes` crate (through WASM), the same
 * picture the desktop editor draws from the same pixels.
 */
export const VIDEO_SCOPES = ["waveform", "parade", "vectorscope", "histogram"] as const;
export type VideoScopeKind = (typeof VIDEO_SCOPES)[number];

export function isVideoScope(value: unknown): value is VideoScopeKind {
	return typeof value === "string" && (VIDEO_SCOPES as readonly string[]).includes(value);
}

/** A drawn scope and what it measured. */
export interface ScopeResult {
	scope: VideoScopeKind;
	width: number;
	height: number;
	rgba: Uint8ClampedArray;
	/** Samples read from the frame. */
	samples: number;
	/** Share (0 to 1) of samples with a channel at 0. */
	atBlack: number;
	/** Share of samples with a channel at 255. */
	atWhite: number;
	/** Luma (0 to 255) the darkest 1% of samples sit at or below. */
	blackLevel: number;
	/** Luma the brightest 1% sit at or above. */
	whiteLevel: number;
}

/**
 * Widest frame read back from the preview; the crate samples at most 384
 * columns, so a wider read only costs time.
 */
export const SCOPE_READ_WIDTH = 768;
/** While playing, the scope is drawn at most this often (ms). */
export const SCOPE_PLAYING_INTERVAL_MS = 100;

/** Draws a scope of RGBA pixels. */
export function drawScope({
	scope,
	rgba,
	width,
	height,
}: {
	scope: VideoScopeKind;
	rgba: Uint8Array | Uint8ClampedArray;
	width: number;
	height: number;
}): ScopeResult {
	const bytes = rgba instanceof Uint8Array ? rgba : new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength);
	const drawn = drawVideoScope(scope, bytes, width, height);
	try {
		return {
			scope,
			width: drawn.width,
			height: drawn.height,
			rgba: new Uint8ClampedArray(drawn.rgba),
			samples: drawn.samples,
			atBlack: drawn.atBlack,
			atWhite: drawn.atWhite,
			blackLevel: drawn.blackLevel,
			whiteLevel: drawn.whiteLevel,
		};
	} finally {
		drawn.free();
	}
}

let readCanvas: HTMLCanvasElement | null = null;

/**
 * Reads a canvas (the preview's output, right after it was drawn) at up to
 * {@link SCOPE_READ_WIDTH} wide and draws a scope of it.
 */
export function scopeOfCanvas({ scope, source }: { scope: VideoScopeKind; source: HTMLCanvasElement }): ScopeResult | null {
	if (source.width === 0 || source.height === 0) return null;
	const width = Math.min(source.width, SCOPE_READ_WIDTH);
	const height = Math.max(1, Math.round((source.height * width) / source.width));
	readCanvas ??= document.createElement("canvas");
	if (readCanvas.width !== width) readCanvas.width = width;
	if (readCanvas.height !== height) readCanvas.height = height;
	const context = readCanvas.getContext("2d", { willReadFrequently: true });
	if (!context) return null;
	context.clearRect(0, 0, width, height);
	context.drawImage(source, 0, 0, width, height);
	return drawScope({ scope, rgba: context.getImageData(0, 0, width, height).data, width, height });
}

interface ScopeState {
	/** The scope shown beside the preview, if any. */
	scope: VideoScopeKind | null;
	/** The scope drawn from the last preview frame. */
	result: ScopeResult | null;
	setScope: (scope: VideoScopeKind | null) => void;
}

export const useScopeStore = create<ScopeState>()((set) => ({
	scope: null,
	result: null,
	setScope: (scope) => set((state) => ({ scope, result: state.result?.scope === scope ? state.result : null })),
}));

let lastCapture = 0;

/**
 * Called by the preview after each frame it draws: draws the open scope of
 * it (at most every {@link SCOPE_PLAYING_INTERVAL_MS} while playing).
 */
export function captureScope({ source, playing, now = performance.now() }: { source: HTMLCanvasElement; playing: boolean; now?: number }): void {
	const { scope } = useScopeStore.getState();
	if (!scope) return;
	if (playing && now - lastCapture < SCOPE_PLAYING_INTERVAL_MS) return;
	lastCapture = now;
	try {
		const result = scopeOfCanvas({ scope, source });
		if (result && useScopeStore.getState().scope === scope) useScopeStore.setState({ result });
	} catch (error) {
		console.warn("Video scope failed", error);
	}
}

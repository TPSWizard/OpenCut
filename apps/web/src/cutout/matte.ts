/**
 * Turning per-frame person confidence into a cut-out matte: a soft
 * threshold (so edges stay smooth without letting low-confidence
 * background through) and a short temporal blur over neighbouring frames,
 * which steadies the flicker a per-frame segmenter has at the edges.
 */

/** Matte width in pixels; the height follows the source's shape. */
export const MATTE_WIDTH = 256;

export function matteSize({ sourceWidth, sourceHeight }: { sourceWidth: number; sourceHeight: number }): {
	width: number;
	height: number;
} {
	const height = Math.max(16, Math.round((MATTE_WIDTH * sourceHeight) / Math.max(1, sourceWidth) / 2) * 2);
	return { width: MATTE_WIDTH, height };
}

/** Confidence 0..1 to matte brightness 0..255 through a smoothstep between `low` and `high`. */
export function confidenceToLuma({
	confidence,
	low = 0.3,
	high = 0.7,
}: {
	confidence: Float32Array | number[];
	low?: number;
	high?: number;
}): Uint8Array {
	const out = new Uint8Array(confidence.length);
	const span = Math.max(1e-6, high - low);
	for (let i = 0; i < confidence.length; i++) {
		const t = Math.min(1, Math.max(0, (confidence[i] - low) / span));
		out[i] = Math.round(t * t * (3 - 2 * t) * 255);
	}
	return out;
}

/**
 * A 1-2-1 blur across frames with one frame of delay: `push` returns the
 * smoothed previous frame (null for the first), `flush` the last one. The
 * first and last frames count their single neighbour twice.
 */
export class TemporalSmoother {
	private previous: Uint8Array | null = null;
	private current: Uint8Array | null = null;

	push(frame: Uint8Array): Uint8Array | null {
		const out = this.current ? blend(this.previous ?? this.current, this.current, frame) : null;
		this.previous = this.current;
		this.current = frame;
		return out;
	}

	flush(): Uint8Array | null {
		if (!this.current) return null;
		const out = blend(this.previous ?? this.current, this.current, this.current);
		this.previous = null;
		this.current = null;
		return out;
	}
}

function blend(a: Uint8Array, b: Uint8Array, c: Uint8Array): Uint8Array {
	const out = new Uint8Array(b.length);
	for (let i = 0; i < b.length; i++) out[i] = Math.round((a[i] + 2 * b[i] + c[i]) / 4);
	return out;
}

/** Share of the matte that is kept (brighter than half). */
export function keptShare({ luma }: { luma: Uint8Array }): number {
	if (luma.length === 0) return 0;
	let kept = 0;
	for (const value of luma) if (value >= 128) kept += 1;
	return kept / luma.length;
}

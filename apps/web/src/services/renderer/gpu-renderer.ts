import {
	applyEffectPasses,
	applyMaskFeather as applyMaskFeatherWasm,
	initializeGpu,
	registerEffectLut,
} from "opencut-wasm";
import type { EffectPass, EffectUniformValue } from "@/effects/types";

let gpuAvailable = false;
let initPromise: Promise<void> | null = null;

export function initializeGpuRenderer(): Promise<void> {
	if (!initPromise) {
		initPromise = initializeGpu()
			.then(() => {
				gpuAvailable = true;
			})
			.catch((error: unknown) => {
				gpuAvailable = false;
				const message = error instanceof Error ? error.message : String(error);
				console.warn(`GPU renderer unavailable: ${message}`);
			});
	}
	return initPromise;
}

export function isGpuAvailable(): boolean {
	return gpuAvailable;
}

export const gpuRenderer = {
	applyEffect({
		source,
		width,
		height,
		passes,
	}: {
		source: OffscreenCanvas;
		width: number;
		height: number;
		passes: EffectPass[];
	}): OffscreenCanvas {
		if (passes.length === 0 || !gpuAvailable) {
			return source;
		}

		for (const pass of passes) {
			if (pass.lut) ensureLutRegistered(pass.lut);
		}
		return applyEffectPasses({
			source,
			width,
			height,
			passes: serializeEffectPasses(passes),
		});
	},

	applyMaskFeather({
		maskCanvas,
		width,
		height,
		feather,
	}: {
		maskCanvas: OffscreenCanvas;
		width: number;
		height: number;
		feather: number;
	}): OffscreenCanvas {
		if (!gpuAvailable) {
			return maskCanvas;
		}

		return applyMaskFeatherWasm({
			mask: maskCanvas,
			width,
			height,
			feather,
		});
	},
};

/** Keys of the look-up tables already uploaded to the GPU runtime. */
const registeredLuts = new Set<string>();

function ensureLutRegistered(lut: NonNullable<EffectPass["lut"]>): void {
	if (!gpuAvailable || registeredLuts.has(lut.key)) return;
	registerEffectLut({ key: lut.key, size: lut.size, data: lut.data });
	registeredLuts.add(lut.key);
}

/**
 * A copy of a compositor frame with every effect pass's table payload
 * replaced by its key, the table uploaded on first sight. The WASM side
 * expects `lut` to be a string.
 */
export function prepareFrameLuts<T>(frame: T): T {
	const walk = (value: unknown): unknown => {
		if (Array.isArray(value)) return value.map(walk);
		if (value && typeof value === "object" && !(value instanceof Uint8Array) && !(value instanceof OffscreenCanvas)) {
			const record = value as Record<string, unknown>;
			if (typeof record.shader === "string" && record.uniforms && typeof record.uniforms === "object" && record.lut && typeof record.lut === "object") {
				const lut = record.lut as NonNullable<EffectPass["lut"]>;
				ensureLutRegistered(lut);
				return { ...record, lut: lut.key };
			}
			let changed = false;
			const out: Record<string, unknown> = {};
			for (const [key, inner] of Object.entries(record)) {
				const next = walk(inner);
				if (next !== inner) changed = true;
				out[key] = next;
			}
			return changed ? out : value;
		}
		return value;
	};
	return walk(frame) as T;
}

function serializeEffectPasses(passes: EffectPass[]) {
	return passes.map((pass) => ({
		lut: pass.lut?.key,
		shader: pass.shader,
		uniforms: Object.entries(pass.uniforms).map(([name, value]) => ({
			name,
			value: normalizeUniformValue(value),
		})),
	}));
}

function normalizeUniformValue(value: EffectUniformValue): number[] {
	return typeof value === "number" ? [value] : value;
}

import { StabilisePath, stabiliseDrawnOffset, stabiliseIsStrength } from "opencut-wasm";
import type { ParamValues } from "@/params";

/**
 * Stabilise on a video clip (row 107): `stabilise.strength` (off, light,
 * medium, strong, still) and the clip's measured camera path in
 * `stabilise.path` (tagged with the media it was measured from). Each
 * frame's correction comes from the shared Rust `stabilise` crate through
 * WASM, as on the desktop (`editor::stabilise_correction`), and moves the
 * drawn frame (`steadyTransform`, the desktop's `steady_transform`). The
 * file is not rewritten. Checked against `fixtures/stabilise/stabilise.json`.
 */

export const STABILISE_STRENGTH_PARAM = "stabilise.strength";
export const STABILISE_PATH_PARAM = "stabilise.path";
export const STABILISE_STRENGTHS = ["off", "light", "medium", "strong", "still"] as const;
export type StabiliseStrength = (typeof STABILISE_STRENGTHS)[number];

export function isStabiliseStrength(value: unknown): value is StabiliseStrength {
	return typeof value === "string" && (STABILISE_STRENGTHS as readonly string[]).includes(value) && stabiliseIsStrength(value);
}

export function readStabiliseStrength(params: ParamValues | undefined): StabiliseStrength {
	const value = params?.[STABILISE_STRENGTH_PARAM];
	return isStabiliseStrength(value) ? value : "off";
}

/** Parsed paths by their stored text; a few clips' worth. */
const parsed = new Map<string, StabilisePath | null>();
const PARSED_KEEP = 16;

function parsePath(text: string): StabilisePath | null {
	if (parsed.has(text)) return parsed.get(text) ?? null;
	let path: StabilisePath | null = null;
	try {
		path = new StabilisePath(text);
	} catch {
		path = null;
	}
	parsed.set(text, path);
	if (parsed.size > PARSED_KEEP) {
		const oldest = parsed.keys().next().value;
		if (oldest !== undefined) {
			parsed.get(oldest)?.free();
			parsed.delete(oldest);
		}
	}
	return path;
}

/** The clip's camera path when it was measured from `mediaId`. */
export function readStabilisePath(params: ParamValues | undefined, mediaId: string): StabilisePath | null {
	const text = params?.[STABILISE_PATH_PARAM];
	if (typeof text !== "string" || text.length === 0) return null;
	const path = parsePath(text);
	return path && path.mediaId() === mediaId ? path : null;
}

/** The zoom for the clip's strength, and the zoom that would hide every edge. */
export function stabiliseZoom(params: ParamValues | undefined, mediaId: string): { zoom: number; needed: number } | null {
	const strength = readStabiliseStrength(params);
	const path = readStabilisePath(params, mediaId);
	if (!path || strength === "off") return null;
	const [zoom, needed] = path.zoom(strength);
	return { zoom, needed };
}

/** What a video node needs to stabilise, or null when it is off or unmeasured. */
export function stabiliseParams(
	params: ParamValues | undefined,
	mediaId: string,
): { strength: StabiliseStrength; path: string } | null {
	const strength = readStabiliseStrength(params);
	if (strength === "off" || !readStabilisePath(params, mediaId)) return null;
	return { strength, path: params?.[STABILISE_PATH_PARAM] as string };
}

/** A correction `[a, b, tx, ty]` (frame units; see `crates/stabilise`). */
export type SteadyCorrection = readonly [number, number, number, number];

/** The frame's correction at a source time, or null when it leaves the frame as it is. */
export function steadyCorrectionAt({
	stabilise,
	sourceSeconds,
}: {
	stabilise: { strength: StabiliseStrength; path: string };
	sourceSeconds: number;
}): SteadyCorrection | null {
	const path = parsePath(stabilise.path);
	if (!path) return null;
	const [a, b, tx, ty] = path.correctionAt(stabilise.strength, sourceSeconds);
	const moved = Math.abs(a - 1) > 1e-9 || Math.abs(b) > 1e-9 || Math.abs(tx) > 1e-9 || Math.abs(ty) > 1e-9;
	return moved ? [a, b, tx, ty] : null;
}

interface SteadyTransform {
	position: { x: number; y: number };
	scaleX: number;
	scaleY: number;
	rotate: number;
}

/**
 * `transform` with the frame moved by `correction`, for a frame drawn
 * `baseWidth` canvas pixels wide at scale 1 (the desktop's `steady_transform`).
 */
export function steadyTransform<T extends SteadyTransform>(transform: T, correction: SteadyCorrection, baseWidth: number): T {
	const [localX, localY, turn, scale] = stabiliseDrawnOffset(
		correction[0],
		correction[1],
		correction[2],
		correction[3],
		baseWidth * Math.abs(transform.scaleX),
		baseWidth * Math.abs(transform.scaleY),
		transform.scaleX < 0,
		transform.scaleY < 0,
	);
	const angle = (transform.rotate * Math.PI) / 180;
	return {
		...transform,
		position: {
			x: transform.position.x + localX * Math.cos(angle) - localY * Math.sin(angle),
			y: transform.position.y + localX * Math.sin(angle) + localY * Math.cos(angle),
		},
		scaleX: transform.scaleX * scale,
		scaleY: transform.scaleY * scale,
		rotate: transform.rotate + turn,
	};
}

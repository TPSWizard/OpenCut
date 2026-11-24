/**
 * A clip's face track (`faces.track`, the shared `facetrack` crate through
 * WASM) as the renderer needs it: which clips have one from their media,
 * and the retouch passes filled with the layer's face ovals. Finding faces
 * is in `./faces` (it loads MediaPipe, so the renderer does not import it).
 */

import { faceTrackFoundCount, faceTrackMedia, retouchFaceUniforms } from "opencut-wasm";
import { RETOUCH_FACE_UNIFORMS } from "@/effects/definitions/library";
import type { EffectPass } from "@/effects/types";
import type { ParamValues } from "@/params";
import type { QuadTransformDescriptor } from "@/services/renderer/compositor/types";

export const FACE_TRACK_PARAM = "faces.track";
/** Looks a second (`facetrack::FACE_SAMPLE_RATE`). */
export const FACE_SAMPLE_RATE = 5;
/** The clip's face track when it was found in `mediaId`, else null. */
export function faceTrackFor(params: ParamValues | undefined, mediaId: string): string | null {
	const track = params?.[FACE_TRACK_PARAM];
	return typeof track === "string" && track.length > 0 && faceTrackMedia(track) === mediaId ? track : null;
}

export function faceTrackFound(track: string): number {
	return faceTrackFoundCount(track);
}

/**
 * The passes with faces-only ones given the layer's face ovals (or left
 * out where there are none); other passes are untouched.
 */
export function withFaceOvals(
	groups: EffectPass[][],
	faces: number[] | null | undefined,
	transform: QuadTransformDescriptor | null,
): EffectPass[][] {
	return groups
		.map((group) =>
			group.flatMap((pass) => {
				if (!pass.faces) return [pass];
				if (!faces || faces.length < 4 || !transform) return [];
				const values = retouchFaceUniforms(
					new Float64Array(faces),
					transform.centerX,
					transform.centerY,
					transform.width,
					transform.height,
					transform.rotationDegrees,
					transform.flipX,
					transform.flipY,
				);
				const uniforms = { ...pass.uniforms };
				RETOUCH_FACE_UNIFORMS.forEach((name, index) => {
					uniforms[name] = values[index];
				});
				return [{ shader: pass.shader, uniforms, ...(pass.lut && { lut: pass.lut }) }];
			}),
		)
		.filter((group) => group.length > 0);
}

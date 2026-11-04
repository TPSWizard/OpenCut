import type { MediaTime } from "@/wasm";

/** A transition parameter as the project stores it: numbers for `float` and
 * `int` uniforms, arrays for vectors, booleans for `bool`. */
export type TransitionParamValue = number | boolean | number[];
export type TransitionParams = Record<string, TransitionParamValue>;

/**
 * The transition a visual clip leads out of, centred on the cut to the next
 * clip on its track (or fading out to nothing when nothing follows it).
 * `type` is a gl-transitions name from the catalogue (`Fade`,
 * `DirectionalWipe`, …); the desktop editor reads the same field.
 */
export interface ElementTransition {
	type: string;
	duration: MediaTime;
	params?: TransitionParams;
}

export type TransitionParamKind =
	| "float"
	| "int"
	| "bool"
	| "vec2"
	| "vec3"
	| "vec4"
	| "ivec2"
	| "ivec3"
	| "ivec4";

/** One user-settable uniform of a transition, as the compositor reports it. */
export interface TransitionParamInfo {
	name: string;
	kind: TransitionParamKind;
	default: TransitionParamValue;
	min?: number | null;
	max?: number | null;
	step?: number | null;
	/** The value is a colour (RGB or RGBA in 0..1). */
	color: boolean;
	description?: string | null;
}

export interface TransitionInfo {
	name: string;
	author: string;
	license: string;
	description?: string | null;
	tags: string[];
	params: TransitionParamInfo[];
}

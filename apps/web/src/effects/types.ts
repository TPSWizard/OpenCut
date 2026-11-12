import type { ParamDefinition, ParamValues } from "@/params";

export interface Effect {
	id: string;
	type: string;
	params: ParamValues;
	enabled: boolean;
}

export type EffectUniformValue = number | number[];

export interface EffectPass {
	shader: string;
	uniforms: Record<string, EffectUniformValue>;
	/** A look-up table the pass reads (the `lut` shader): registered once by key, then named. */
	lut?: { key: string; size: number; data: Uint8Array };
}

/** What a pass builder gets: the effect's params, the frame size and the
 * time into the element in seconds (for effects that move). */
export interface EffectPassContext {
	effectParams: ParamValues;
	width: number;
	height: number;
	timeSeconds: number;
}

export interface EffectPassTemplate {
	shader: string;
	uniforms(params: EffectPassContext): Record<string, EffectUniformValue>;
}

export interface EffectRendererConfig {
	passes: EffectPassTemplate[];
	buildPasses?: (params: EffectPassContext) => EffectPass[];
}

export interface EffectDefinition {
	type: string;
	name: string;
	keywords: string[];
	params: ParamDefinition[];
	renderer: EffectRendererConfig;
}

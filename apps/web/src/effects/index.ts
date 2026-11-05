import { generateUUID } from "@/utils/id";
import { buildDefaultParamValues } from "@/params/registry";
import { effectsRegistry } from "./registry";
import type { ParamValues } from "@/params";
import type { Effect, EffectDefinition, EffectPass } from "@/effects/types";
import { VISUAL_ELEMENT_TYPES } from "@/timeline";

export { effectsRegistry } from "./registry";
export { registerDefaultEffects } from "./definitions";

export function resolveEffectPasses({
	definition,
	effectParams,
	width,
	height,
	timeSeconds = 0,
}: {
	definition: EffectDefinition;
	effectParams: ParamValues;
	width: number;
	height: number;
	/** Time into the element, for effects that move (grain, glitch). */
	timeSeconds?: number;
}): EffectPass[] {
	const context = { effectParams, width, height, timeSeconds };
	if (definition.renderer.buildPasses) {
		return definition.renderer.buildPasses(context);
	}
	return definition.renderer.passes.map((pass) => ({
		shader: pass.shader,
		uniforms: pass.uniforms(context),
	}));
}

export const EFFECT_TARGET_ELEMENT_TYPES = VISUAL_ELEMENT_TYPES;

export function buildDefaultEffectInstance({
	effectType,
}: {
	effectType: string;
}): Effect {
	const definition = effectsRegistry.get(effectType);
	const params: ParamValues = buildDefaultParamValues(definition.params);

	return {
		id: generateUUID(),
		type: effectType,
		params,
		enabled: true,
	};
}

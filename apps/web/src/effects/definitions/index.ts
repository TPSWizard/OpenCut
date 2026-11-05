import { effectsRegistry } from "../registry";
import { blurEffectDefinition } from "./blur";
import { libraryEffectDefinitions } from "./library";

const defaultEffects = [blurEffectDefinition, ...libraryEffectDefinitions];

export function registerDefaultEffects(): void {
	for (const definition of defaultEffects) {
		if (effectsRegistry.has(definition.type)) {
			continue;
		}
		effectsRegistry.register({
			key: definition.type,
			definition,
		});
	}
}

import { getPersistedKeybindingsState } from "../persisted-state";

/**
 * Version 9 adds "select range" (Ctrl+Shift+A selects every clip between
 * the in and out marks). Its key is added where it is still free.
 */
const ADDED_DEFAULTS: Record<string, string> = {
	"ctrl+shift+a": "select-range",
};

export function v8ToV9({ state }: { state: unknown }): unknown {
	const v8 = getPersistedKeybindingsState({ state });
	if (!v8) return state;
	const keybindings = { ...v8.keybindings };
	for (const [key, action] of Object.entries(ADDED_DEFAULTS)) {
		if (keybindings[key] === undefined) keybindings[key] = action;
	}
	return { ...v8, keybindings };
}

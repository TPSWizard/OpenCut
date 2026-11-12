import { getPersistedKeybindingsState } from "../persisted-state";

/**
 * Version 10 adds grouping and compound clips: Ctrl+G groups, Ctrl+Shift+G
 * ungroups, Alt+G makes a compound clip and Alt+Shift+G breaks one apart.
 * Each key is added where it is still free.
 */
const ADDED_DEFAULTS: Record<string, string> = {
	"ctrl+g": "group-selected",
	"ctrl+shift+g": "ungroup-selected",
	"alt+g": "create-compound",
	"alt+shift+g": "break-apart-compound",
};

export function v9ToV10({ state }: { state: unknown }): unknown {
	const v9 = getPersistedKeybindingsState({ state });
	if (!v9) return state;
	const keybindings = { ...v9.keybindings };
	for (const [key, action] of Object.entries(ADDED_DEFAULTS)) {
		if (keybindings[key] === undefined) keybindings[key] = action;
	}
	return { ...v9, keybindings };
}

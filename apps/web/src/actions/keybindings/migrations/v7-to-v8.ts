import { getPersistedKeybindingsState } from "../persisted-state";

/**
 * Version 8 adds marks, ripple delete, edit-point navigation, the bookmark
 * key and the Ctrl+B split alias. Their default keys are added to a saved
 * map wherever the key is still free, so an existing user gets them too.
 */
const ADDED_DEFAULTS: Record<string, string> = {
	"ctrl+b": "split",
	m: "toggle-bookmark",
	i: "mark-in",
	o: "mark-out",
	"shift+i": "goto-mark-in",
	"shift+o": "goto-mark-out",
	"ctrl+shift+x": "clear-marks",
	"shift+delete": "ripple-delete",
	"shift+backspace": "ripple-delete",
	"alt+delete": "delete-marked-range",
	down: "next-edit",
	up: "previous-edit",
};

export function v7ToV8({ state }: { state: unknown }): unknown {
	const v7 = getPersistedKeybindingsState({ state });
	if (!v7) return state;
	const keybindings = { ...v7.keybindings };
	for (const [key, action] of Object.entries(ADDED_DEFAULTS)) {
		if (keybindings[key] === undefined) keybindings[key] = action;
	}
	return { ...v7, keybindings };
}

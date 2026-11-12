/**
 * Shortcut presets: the editor's own map and maps that follow the habits
 * of Premiere Pro, Final Cut Pro and DaVinci Resolve for the actions this
 * editor has. Each preset starts from the defaults and overrides the keys
 * listed, so an action a preset does not mention keeps its default. The
 * other editors' keys are taken from their published shortcut lists and
 * only cover actions both editors share; keys this editor cannot bind
 * (`'`, `;`, `[`, `]`, function keys) are left out.
 */

import type { ShortcutKey } from "@/actions/keybinding";
import { getDefaultShortcuts } from "@/actions/definitions";
import type { TActionWithOptionalArgs } from "@/actions/types";

export interface KeybindingPreset {
	id: string;
	label: string;
	description: string;
	/** Keys per action; replaces the action's default keys. */
	overrides: Partial<Record<TActionWithOptionalArgs, readonly ShortcutKey[]>>;
}

export const KEYBINDING_PRESETS: KeybindingPreset[] = [
	{
		id: "opencut",
		label: "ConzaCut",
		description: "The editor's own shortcuts.",
		overrides: {},
	},
	{
		id: "premiere",
		label: "Premiere Pro",
		description: "Ctrl+K adds an edit, Q and W ripple-trim, Shift+Delete ripple-deletes, Ctrl+Shift+X clears the marks.",
		overrides: {
			split: ["ctrl+k", "ctrl+b"],
			"ripple-delete": ["shift+delete", "shift+backspace"],
			"delete-selected": ["delete", "backspace"],
			"toggle-snapping": ["s"],
			"clear-marks": ["ctrl+shift+x"],
			"jump-forward": ["shift+right"],
			"jump-backward": ["shift+left"],
			"goto-start": ["home"],
			"goto-end": ["end"],
		},
	},
	{
		id: "final-cut",
		label: "Final Cut Pro",
		description: "Delete ripple-deletes (the storyline closes up), Shift+Delete leaves a gap, Ctrl+B blades at the playhead.",
		overrides: {
			split: ["ctrl+b"],
			"ripple-delete": ["delete", "backspace"],
			"delete-selected": ["shift+delete", "shift+backspace"],
			"toggle-snapping": ["n"],
			"clear-marks": ["alt+x"],
			"goto-start": ["home"],
			"goto-end": ["end"],
		},
	},
	{
		id: "resolve",
		label: "DaVinci Resolve",
		description: "Ctrl+B splits, Shift+Delete ripple-deletes, Alt+X clears the marks, N toggles snapping.",
		overrides: {
			split: ["ctrl+b"],
			"ripple-delete": ["shift+delete", "shift+backspace"],
			"delete-selected": ["delete", "backspace"],
			"toggle-snapping": ["n"],
			"clear-marks": ["alt+x"],
			"goto-start": ["home"],
			"goto-end": ["end"],
		},
	},
];

export const DEFAULT_PRESET_ID = KEYBINDING_PRESETS[0].id;

export function findPreset({ id }: { id: string }): KeybindingPreset | undefined {
	return KEYBINDING_PRESETS.find((preset) => preset.id === id);
}

/** The full key map of a preset: the defaults with the preset's keys in place of the overridden actions'. */
export function presetShortcuts({ preset }: { preset: KeybindingPreset }): Map<ShortcutKey, TActionWithOptionalArgs> {
	const map = getDefaultShortcuts();
	const overridden = new Set(Object.keys(preset.overrides));
	for (const [key, action] of [...map]) {
		if (overridden.has(action)) map.delete(key);
	}
	// A preset's key wins over a default that happened to sit on it.
	for (const [action, keys] of Object.entries(preset.overrides)) {
		for (const key of keys ?? []) {
			map.set(key, action as TActionWithOptionalArgs);
		}
	}
	return map;
}

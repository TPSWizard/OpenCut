/**
 * The resize choice the settings panel keeps for this browser: how a new
 * canvas size re-fits the content, or "none" to change the canvas only.
 */

import type { TProjectSettings } from "@/project/types";
import type { EditorCore } from "@/core";
import { resizeProject } from "./apply";
import { RESIZE_MODES, type ResizeMode } from "./plan";

export type ContentChoice = ResizeMode | "none";
const CONTENT_CHOICE_KEY = "opencut:resize-content";

export function readContentChoice(): ContentChoice {
	try {
		const stored = localStorage.getItem(CONTENT_CHOICE_KEY);
		if (stored === "none" || RESIZE_MODES.includes(stored as ResizeMode)) return stored as ContentChoice;
	} catch {
		// Storage can be unavailable; the default applies.
	}
	return "fit";
}

export function saveContentChoice(choice: ContentChoice): void {
	try {
		localStorage.setItem(CONTENT_CHOICE_KEY, choice);
	} catch {
		// Not kept without storage.
	}
}

/** Applies project settings; a new canvas size re-fits the content as `choice` says. */
export function applyCanvasSettings({
	editor,
	settings,
	choice = readContentChoice(),
}: {
	editor: EditorCore;
	settings: Partial<TProjectSettings>;
	choice?: ContentChoice;
}): void {
	const next = settings.canvasSize;
	const current = editor.project.getActive()?.settings.canvasSize;
	const changes = next && current && (next.width !== current.width || next.height !== current.height);
	if (changes && choice !== "none") {
		try {
			resizeProject({ editor, canvas: next, mode: choice, settings });
			return;
		} catch (error) {
			console.error("Resize failed", error);
		}
	}
	void editor.project.updateSettings({ settings });
}

import { useSyncExternalStore } from "react";
import type { EditorApiBridge } from "./bridge";

/**
 * The currently installed Editor API bridge, observable from React. Plugin
 * hosts and the scripting runner use this rather than reaching for
 * `window.opencut`, so they also work before the global is attached and in
 * environments without a window.
 */
let current: EditorApiBridge | null = null;
const listeners = new Set<() => void>();

export function setCurrentEditorApiBridge(bridge: EditorApiBridge | null): void {
	current = bridge;
	for (const listener of listeners) listener();
}

export function getCurrentEditorApiBridge(): EditorApiBridge | null {
	return current;
}

function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

export function useEditorApiBridge(): EditorApiBridge | null {
	return useSyncExternalStore(
		subscribe,
		getCurrentEditorApiBridge,
		() => null,
	);
}

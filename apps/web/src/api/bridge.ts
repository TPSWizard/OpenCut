import type { EditorCore } from "@/core";
import type { RemoteNamespaces } from "./client";
import { createEditorApi, type EditorApi } from "./editor-api";
import { setCurrentEditorApiBridge } from "./bridge-store";
import {
	EDITOR_API_VERSION,
	EditorApiError,
	isApiMessage,
	type ApiEventMessage,
	type ApiMessage,
	type ApiRequestMessage,
	type ApiResponseMessage,
	type EditorApiEventName,
	type EditorApiMethodName,
	type EditorApiParams,
	type EditorApiResult,
} from "./types";

/**
 * What the editor exposes on `window.opencut`. Scripts running in the page
 * (devtools, the headless runner through Playwright) use this directly;
 * Workers and iframes go through `handleMessage`.
 */
export interface EditorApiBridge extends LocalNamespaces {
	apiVersion: string;
	api: EditorApi;
	call: <M extends EditorApiMethodName>(
		method: M,
		params?: EditorApiParams<M>,
	) => Promise<EditorApiResult<M>>;
	on: (
		event: EditorApiEventName | "*",
		listener: (event: ApiEventMessage) => void,
	) => () => void;
	/**
	 * Serve a request that arrived over postMessage. Returns the response and
	 * the buffers that should be transferred with it.
	 */
	handleMessage: (
		message: unknown,
	) => Promise<{ response: ApiResponseMessage; transfer: Transferable[] } | null>;
	/** Allow a window (plugin iframe) to send requests through `window.postMessage`. */
	allowSource: (source: Window) => () => void;
	dispose: () => void;
}

/** `window.opencut.timeline.get()` style access, served by the local API. */
export type LocalNamespaces = RemoteNamespaces;

declare global {
	interface Window {
		opencut?: EditorApiBridge;
	}
}

function toErrorPayload(error: unknown): ApiResponseMessage["error"] {
	if (error instanceof EditorApiError) {
		return { code: error.code, message: error.message };
	}
	return {
		code: "failed",
		message: error instanceof Error ? error.message : String(error),
	};
}

function collectTransferables(result: unknown): Transferable[] {
	if (
		typeof result === "object" &&
		result !== null &&
		"bytes" in result &&
		(result as { bytes: unknown }).bytes instanceof ArrayBuffer
	) {
		return [(result as { bytes: ArrayBuffer }).bytes];
	}
	return [];
}

/**
 * Install the Editor API bridge for the lifetime of an open project. Returns
 * the bridge; call `dispose` when the project closes.
 */
export function installEditorApiBridge({
	editor,
	target = typeof window === "undefined" ? null : window,
}: {
	editor: EditorCore;
	target?: Window | null;
}): EditorApiBridge {
	const api = createEditorApi({ editor });
	const listeners = new Map<string, Set<(event: ApiEventMessage) => void>>();
	const allowedSources = new Set<Window>();
	let sequence = 0;
	const disposers: Array<() => void> = [];

	const emit = (event: EditorApiEventName) => {
		sequence += 1;
		const message: ApiEventMessage = {
			type: "opencut:event",
			event,
			sequence,
		};
		for (const key of [event, "*"]) {
			const set = listeners.get(key);
			if (!set) continue;
			for (const listener of set) listener(message);
		}
		for (const source of allowedSources) {
			try {
				source.postMessage(message, "*");
			} catch {
				allowedSources.delete(source);
			}
		}
	};

	// Coalesce bursts of manager notifications into one event per tick.
	const scheduled = new Set<EditorApiEventName>();
	let flushQueued = false;
	const schedule = (event: EditorApiEventName) => {
		scheduled.add(event);
		if (flushQueued) return;
		flushQueued = true;
		queueMicrotask(() => {
			flushQueued = false;
			const events = [...scheduled];
			scheduled.clear();
			for (const item of events) emit(item);
		});
	};

	disposers.push(
		editor.project.subscribe(() => schedule("project")),
		editor.scenes.subscribe(() => schedule("timeline")),
		editor.timeline.subscribe(() => schedule("timeline")),
		editor.media.subscribe(() => schedule("media")),
		editor.playback.subscribe(() => schedule("playback")),
		editor.selection.subscribe(() => schedule("selection")),
	);

	const call: EditorApiBridge["call"] = (method, params) =>
		api.dispatch(method, (params ?? {}) as EditorApiParams<typeof method>);

	const handleMessage: EditorApiBridge["handleMessage"] = async (message) => {
		if (!isApiMessage(message) || message.type !== "opencut:request") {
			return null;
		}
		const request = message as ApiRequestMessage;
		try {
			const result = await api.dispatch(
				request.method as EditorApiMethodName,
				(request.params ?? {}) as never,
			);
			return {
				response: {
					type: "opencut:response",
					id: request.id,
					ok: true,
					result,
				},
				transfer: collectTransferables(result),
			};
		} catch (error) {
			return {
				response: {
					type: "opencut:response",
					id: request.id,
					ok: false,
					error: toErrorPayload(error),
				},
				transfer: [],
			};
		}
	};

	const onWindowMessage = (event: MessageEvent) => {
		const source = event.source;
		if (!source || !allowedSources.has(source as Window)) return;
		void handleMessage(event.data).then((reply) => {
			if (!reply) return;
			(source as Window).postMessage(reply.response, "*", reply.transfer);
		});
	};
	target?.addEventListener("message", onWindowMessage);

	const base: Omit<EditorApiBridge, keyof LocalNamespaces> = {
		apiVersion: EDITOR_API_VERSION,
		api,
		call,
		on: (event, listener) => {
			let set = listeners.get(event);
			if (!set) {
				set = new Set();
				listeners.set(event, set);
			}
			set.add(listener);
			return () => {
				set?.delete(listener);
			};
		},
		handleMessage,
		allowSource: (source) => {
			allowedSources.add(source);
			try {
				source.postMessage(
					{ type: "opencut:ready", apiVersion: EDITOR_API_VERSION } satisfies ApiMessage,
					"*",
				);
			} catch {
				// The frame may not be ready yet; it can still send requests later.
			}
			return () => {
				allowedSources.delete(source);
			};
		},
		dispose: () => {
			for (const dispose of disposers) dispose();
			target?.removeEventListener("message", onWindowMessage);
			listeners.clear();
			allowedSources.clear();
			if (target && target.opencut === bridge) {
				delete target.opencut;
			}
			setCurrentEditorApiBridge(null);
		},
	};

	// Namespaced access (`bridge.timeline.get()`) is proxied onto `call`.
	const bridge = new Proxy(base as EditorApiBridge, {
		get(target, prop) {
			if (prop in target) return target[prop as keyof typeof target];
			if (typeof prop !== "string" || prop === "then") return undefined;
			if (!api.methods.some((method) => method.startsWith(`${prop}.`))) {
				return undefined;
			}
			return new Proxy(
				{},
				{
					get(_inner, fn) {
						if (typeof fn !== "string" || fn === "then") return undefined;
						return (params?: unknown) =>
							call(`${prop}.${fn}` as EditorApiMethodName, (params ?? {}) as never);
					},
				},
			);
		},
	});

	setCurrentEditorApiBridge(bridge);
	if (target) {
		target.opencut = bridge;
		target.dispatchEvent(new CustomEvent("opencut:ready", { detail: bridge }));
	}

	return bridge;
}

/**
 * Wait for `window.opencut` from page code (used by the headless runner and
 * by devtools snippets).
 */
export function waitForEditorApi({
	target = window,
	timeoutMs = 60_000,
}: {
	target?: Window;
	timeoutMs?: number;
} = {}): Promise<EditorApiBridge> {
	if (target.opencut) return Promise.resolve(target.opencut);
	return new Promise((resolve, reject) => {
		const timer = setTimeout(() => {
			target.removeEventListener("opencut:ready", onReady);
			reject(new Error("Editor API did not become ready in time"));
		}, timeoutMs);
		const onReady = (event: Event) => {
			clearTimeout(timer);
			resolve((event as CustomEvent<EditorApiBridge>).detail);
		};
		target.addEventListener("opencut:ready", onReady, { once: true });
	});
}

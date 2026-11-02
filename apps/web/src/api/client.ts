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
 * A transport the remote client talks over: a Worker, an iframe's parent
 * window, or anything else that can post structured-cloneable messages.
 */
export interface ApiTransport {
	post: (message: ApiMessage, transfer?: Transferable[]) => void;
	subscribe: (listener: (message: ApiMessage) => void) => () => void;
}

export interface RemoteEditorApi {
	/** Call any method by its dotted name. */
	call: <M extends EditorApiMethodName>(
		method: M,
		params?: EditorApiParams<M>,
	) => Promise<EditorApiResult<M>>;
	/** Listen for editor change notifications. Returns an unsubscribe fn. */
	on: (
		event: EditorApiEventName | "*",
		listener: (event: ApiEventMessage) => void,
	) => () => void;
	/** Resolves once the editor side has announced itself. */
	ready: () => Promise<void>;
	/** `opencut.timeline.addClip({...})` style access, generated from method names. */
	namespaces: RemoteNamespaces;
	dispose: () => void;
}

type NamespaceOf<M extends string> = M extends `${infer N}.${string}` ? N : never;
type MethodOf<N extends string, M extends string> = M extends `${N}.${infer F}`
	? F
	: never;

export type RemoteNamespaces = {
	[N in NamespaceOf<EditorApiMethodName>]: {
		[M in Extract<EditorApiMethodName, `${N}.${string}`> as MethodOf<N, M>]: (
			params?: EditorApiParams<M>,
		) => Promise<EditorApiResult<M>>;
	};
};

let requestCounter = 0;

function nextRequestId(): string {
	requestCounter += 1;
	return `${Date.now().toString(36)}-${requestCounter}`;
}

/**
 * Build a client for the Editor API over any message transport. Used by the
 * scripting Worker, by plugin iframes (via the plugin SDK) and by tests.
 */
export function createRemoteEditorApi({
	transport,
	timeoutMs = 120_000,
}: {
	transport: ApiTransport;
	timeoutMs?: number;
}): RemoteEditorApi {
	const pending = new Map<
		string,
		{
			resolve: (value: unknown) => void;
			reject: (error: Error) => void;
			timer: ReturnType<typeof setTimeout> | null;
		}
	>();
	const listeners = new Map<string, Set<(event: ApiEventMessage) => void>>();
	let readyResolve: (() => void) | null = null;
	let isReady = false;
	const readyPromise = new Promise<void>((resolve) => {
		readyResolve = resolve;
	});

	const unsubscribe = transport.subscribe((message) => {
		if (!isApiMessage(message)) return;
		if (message.type === "opencut:ready") {
			isReady = true;
			readyResolve?.();
			return;
		}
		if (message.type === "opencut:event") {
			for (const key of [message.event, "*"]) {
				const set = listeners.get(key);
				if (!set) continue;
				for (const listener of set) listener(message);
			}
			return;
		}
		if (message.type === "opencut:response") {
			const entry = pending.get(message.id);
			if (!entry) return;
			pending.delete(message.id);
			if (entry.timer) clearTimeout(entry.timer);
			if (message.ok) {
				entry.resolve(message.result);
			} else {
				entry.reject(
					new EditorApiError({
						code: message.error?.code ?? "failed",
						message: message.error?.message ?? "Editor API call failed",
					}),
				);
			}
		}
	});

	const call: RemoteEditorApi["call"] = (method, params) =>
		new Promise((resolve, reject) => {
			const id = nextRequestId();
			const timer =
				timeoutMs > 0
					? setTimeout(() => {
							pending.delete(id);
							reject(
								new EditorApiError({
									code: "failed",
									message: `${method} timed out after ${timeoutMs} ms`,
								}),
							);
						}, timeoutMs)
					: null;
			pending.set(id, {
				resolve: resolve as (value: unknown) => void,
				reject,
				timer,
			});
			const request: ApiRequestMessage = {
				type: "opencut:request",
				id,
				method,
				params: params ?? {},
			};
			transport.post(request);
		});

	const on: RemoteEditorApi["on"] = (event, listener) => {
		let set = listeners.get(event);
		if (!set) {
			set = new Set();
			listeners.set(event, set);
		}
		set.add(listener);
		return () => {
			set?.delete(listener);
		};
	};

	const namespaces = new Proxy({} as RemoteNamespaces, {
		get(_target, namespace) {
			if (typeof namespace !== "string") return undefined;
			return new Proxy(
				{},
				{
					get(_inner, fn) {
						if (typeof fn !== "string") return undefined;
						if (fn === "then") return undefined;
						return (params?: unknown) =>
							call(
								`${namespace}.${fn}` as EditorApiMethodName,
								(params ?? {}) as never,
							);
					},
				},
			);
		},
	});

	return {
		call,
		on,
		ready: () => (isReady ? Promise.resolve() : readyPromise),
		namespaces,
		dispose: () => {
			unsubscribe();
			for (const entry of pending.values()) {
				if (entry.timer) clearTimeout(entry.timer);
				entry.reject(
					new EditorApiError({ code: "failed", message: "client disposed" }),
				);
			}
			pending.clear();
		},
	};
}

/**
 * The object handed to user code: `opencut.timeline.addClip(...)` plus
 * `call`, `on` and `apiVersion`. Namespaces are proxied, so this cannot be
 * built with a spread.
 */
export type EditorApiFacade = RemoteNamespaces & {
	call: RemoteEditorApi["call"];
	on: RemoteEditorApi["on"];
	apiVersion: string;
};

export function createEditorApiFacade(api: RemoteEditorApi): EditorApiFacade {
	const base = {
		call: api.call,
		on: api.on,
		apiVersion: EDITOR_API_VERSION,
	};
	return new Proxy(base as EditorApiFacade, {
		get(target, prop) {
			if (prop in target) return target[prop as keyof typeof target];
			if (prop === "then") return undefined;
			return api.namespaces[prop as keyof RemoteNamespaces];
		},
		has(target, prop) {
			return prop in target || typeof prop === "string";
		},
	});
}

/** Transport for code running inside a Worker spawned by the editor. */
export function workerTransport(scope: {
	postMessage: (message: unknown, transfer: Transferable[]) => void;
	addEventListener: (type: "message", listener: (event: MessageEvent) => void) => void;
	removeEventListener: (
		type: "message",
		listener: (event: MessageEvent) => void,
	) => void;
}): ApiTransport {
	return {
		post: (message, transfer) => scope.postMessage(message, transfer ?? []),
		subscribe: (listener) => {
			const handler = (event: MessageEvent) => listener(event.data as ApiMessage);
			scope.addEventListener("message", handler);
			return () => scope.removeEventListener("message", handler);
		},
	};
}

/** Transport for a page embedded in the editor as a plugin iframe. */
export function parentWindowTransport({
	target = window.parent,
	targetOrigin = "*",
}: {
	target?: Window;
	targetOrigin?: string;
} = {}): ApiTransport {
	return {
		post: (message, transfer) =>
			target.postMessage(message, targetOrigin, transfer),
		subscribe: (listener) => {
			const handler = (event: MessageEvent) => {
				if (event.source !== target) return;
				listener(event.data as ApiMessage);
			};
			window.addEventListener("message", handler);
			return () => window.removeEventListener("message", handler);
		},
	};
}

export { EDITOR_API_VERSION, EditorApiError };
export type { ApiResponseMessage };

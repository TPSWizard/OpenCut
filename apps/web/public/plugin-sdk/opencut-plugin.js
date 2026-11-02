/**
 * OpenCut plugin SDK (browser ES module, no build step).
 *
 * Load it from a plugin panel page:
 *
 *   <script type="module">
 *     import { connect } from "https://<editor host>/plugin-sdk/opencut-plugin.js";
 *     const opencut = await connect();
 *     const project = await opencut.project.get();
 *   </script>
 *
 * The returned object mirrors the Editor API: `opencut.<namespace>.<method>(params)`
 * returns a promise, `opencut.call("timeline.get")` calls any method by name,
 * `opencut.on("timeline", listener)` subscribes to change events, and
 * `opencut.apiVersion` is the editor's API version.
 *
 * Panels run in a sandboxed iframe with an opaque origin; the editor only
 * answers messages from frames it created, so nothing else on the page can
 * impersonate a plugin.
 */

let counter = 0;

/**
 * @param {{ target?: Window, timeoutMs?: number }} [options]
 */
export function connect(options = {}) {
	const target = options.target ?? window.parent;
	const timeoutMs = options.timeoutMs ?? 60_000;
	const pending = new Map();
	const listeners = new Map();
	let apiVersion = null;

	const onMessage = (event) => {
		if (event.source !== target) return;
		const message = event.data;
		if (!message || typeof message.type !== "string") return;
		if (message.type === "opencut:ready") {
			apiVersion = message.apiVersion;
			readyResolve();
			return;
		}
		if (message.type === "opencut:event") {
			for (const key of [message.event, "*"]) {
				const set = listeners.get(key);
				if (set) for (const listener of set) listener(message);
			}
			return;
		}
		if (message.type === "opencut:response") {
			const entry = pending.get(message.id);
			if (!entry) return;
			pending.delete(message.id);
			clearTimeout(entry.timer);
			if (message.ok) entry.resolve(message.result);
			else {
				const error = new Error(message.error?.message ?? "Editor API call failed");
				error.code = message.error?.code ?? "failed";
				entry.reject(error);
			}
		}
	};

	let readyResolve;
	const ready = new Promise((resolve) => {
		readyResolve = resolve;
	});
	window.addEventListener("message", onMessage);

	const call = (method, params = {}) =>
		new Promise((resolve, reject) => {
			counter += 1;
			const id = `${Date.now().toString(36)}-${counter}`;
			const timer = setTimeout(() => {
				pending.delete(id);
				reject(new Error(`${method} timed out`));
			}, timeoutMs);
			pending.set(id, { resolve, reject, timer });
			target.postMessage({ type: "opencut:request", id, method, params }, "*");
		});

	const on = (event, listener) => {
		if (!listeners.has(event)) listeners.set(event, new Set());
		listeners.get(event).add(listener);
		return () => listeners.get(event)?.delete(listener);
	};

	const api = new Proxy(
		{
			call,
			on,
			get apiVersion() {
				return apiVersion;
			},
			disconnect() {
				window.removeEventListener("message", onMessage);
			},
		},
		{
			get(base, namespace) {
				if (namespace in base) return base[namespace];
				if (typeof namespace !== "string") return undefined;
				return new Proxy(
					{},
					{
						get(_inner, method) {
							if (typeof method !== "string" || method === "then") return undefined;
							return (params) => call(`${namespace}.${method}`, params ?? {});
						},
					},
				);
			},
		},
	);

	// The editor announces itself when it registers the frame; ask anyway in
	// case the frame loaded first.
	return Promise.race([
		ready,
		new Promise((_, reject) =>
			setTimeout(() => reject(new Error("The OpenCut editor did not respond")), timeoutMs),
		),
	]).then(() => api);
}

/// <reference lib="webworker" />
import {
	createEditorApiFacade,
	createRemoteEditorApi,
	workerTransport,
} from "@/api/client";

/**
 * Runtime for a plugin's background `entry` module. The host posts
 * `{ type: "plugin:start", entryUrl }`; this worker imports the module and
 * calls its `activate` (or default export) with the remote Editor API. Any
 * `opencut:*` messages are handled by the API client.
 */
declare const self: DedicatedWorkerGlobalScope;

type PluginModule = {
	default?: unknown;
	activate?: unknown;
	deactivate?: unknown;
};

const api = createRemoteEditorApi({ transport: workerTransport(self) });
let deactivate: (() => unknown) | null = null;

function post(message: unknown) {
	self.postMessage(message);
}

self.addEventListener("message", (event: MessageEvent) => {
	const data = event.data as { type?: string; entryUrl?: string } | null;
	if (!data || typeof data !== "object") return;
	if (data.type === "plugin:start" && typeof data.entryUrl === "string") {
		void start(data.entryUrl);
	} else if (data.type === "plugin:stop") {
		void stop();
	}
});

async function start(entryUrl: string) {
	try {
		const module = (await import(/* @vite-ignore */ entryUrl)) as PluginModule;
		const activate =
			typeof module.activate === "function"
				? module.activate
				: typeof module.default === "function"
					? module.default
					: null;
		if (!activate) {
			throw new Error("The entry module has no activate() or default export");
		}
		const result = await (activate as (api: unknown) => unknown)(
			createEditorApiFacade(api),
		);
		if (typeof result === "function") {
			deactivate = result as () => unknown;
		} else if (typeof module.deactivate === "function") {
			deactivate = module.deactivate as () => unknown;
		}
		post({ type: "plugin:started" });
	} catch (error) {
		post({
			type: "plugin:error",
			message: error instanceof Error ? error.message : String(error),
		});
	}
}

async function stop() {
	try {
		await deactivate?.();
	} finally {
		post({ type: "plugin:stopped" });
	}
}

// Ready for the host's first message; the ready signal for the API itself is
// sent by the host once the bridge is attached.
post({ type: "plugin:loaded" });

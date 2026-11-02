"use client";

import { useEffect, useRef } from "react";
import { useEditorApiBridge } from "@/api/bridge-store";
import type { EditorApiBridge } from "@/api/bridge";
import { EDITOR_API_VERSION } from "@/api/types";
import { useEnabledPlugins, usePluginsStore } from "./plugins-store";
import type { InstalledPlugin, PluginStatus } from "./types";

/**
 * Runs the background `entry` module of every enabled plugin in its own
 * Worker while a project is open, and routes its Editor API requests through
 * the bridge. Rendered once inside the editor.
 */
export function PluginHost() {
	const bridge = useEditorApiBridge();
	const plugins = useEnabledPlugins();
	const setStatus = usePluginsStore((state) => state.setStatus);
	const workersRef = useRef(new Map<string, Worker>());

	const entryPlugins = plugins.filter((plugin) => plugin.entryUrl);
	const entryKey = entryPlugins
		.map((plugin) => `${plugin.manifest.id}@${plugin.entryUrl}`)
		.join("|");

	useEffect(() => {
		if (!bridge) return;
		const workers = workersRef.current;
		const wanted = new Map(entryPlugins.map((plugin) => [plugin.manifest.id, plugin]));

		for (const [pluginId, worker] of workers) {
			if (!wanted.has(pluginId)) {
				worker.terminate();
				workers.delete(pluginId);
				setStatus({ pluginId, status: { state: "idle" } });
			}
		}

		for (const [pluginId, plugin] of wanted) {
			if (workers.has(pluginId)) continue;
			workers.set(pluginId, startPluginWorker({ plugin, bridge, setStatus }));
		}

		return () => {
			for (const [pluginId, worker] of workers) {
				worker.terminate();
				setStatus({ pluginId, status: { state: "idle" } });
			}
			workers.clear();
		};
		// entryKey captures the identity of every entry plugin.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [bridge, entryKey, setStatus]);

	return null;
}

function startPluginWorker({
	plugin,
	bridge,
	setStatus,
}: {
	plugin: InstalledPlugin;
	bridge: EditorApiBridge;
	setStatus: (args: { pluginId: string; status: PluginStatus }) => void;
}): Worker {
	const pluginId = plugin.manifest.id;
	const worker = new Worker(new URL("./plugin-worker.ts", import.meta.url), {
		type: "module",
		name: `opencut-plugin:${pluginId}`,
	});

	worker.addEventListener("message", (event: MessageEvent) => {
		const data = event.data as { type?: string; message?: string } | null;
		if (!data || typeof data !== "object") return;
		switch (data.type) {
			case "plugin:loaded":
				worker.postMessage({ type: "opencut:ready", apiVersion: EDITOR_API_VERSION });
				worker.postMessage({ type: "plugin:start", entryUrl: plugin.entryUrl });
				return;
			case "plugin:started":
				setStatus({ pluginId, status: { state: "running" } });
				return;
			case "plugin:error":
				setStatus({
					pluginId,
					status: { state: "error", message: data.message ?? "Plugin failed" },
				});
				return;
			case "opencut:request":
				void bridge.handleMessage(data).then((reply) => {
					if (reply) worker.postMessage(reply.response, reply.transfer);
				});
				return;
			default:
				return;
		}
	});

	worker.addEventListener("error", (event) => {
		setStatus({
			pluginId,
			status: { state: "error", message: event.message || "Plugin worker crashed" },
		});
	});

	const unsubscribe = bridge.on("*", (event) => {
		try {
			worker.postMessage(event);
		} catch {
			unsubscribe();
		}
	});

	return worker;
}

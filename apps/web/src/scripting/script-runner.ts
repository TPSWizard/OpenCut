import { EDITOR_API_VERSION } from "@/api/types";
import type { EditorApiBridge } from "@/api/bridge";

export type ScriptLogLevel = "log" | "info" | "warn" | "error" | "system";

export interface ScriptLogEntry {
	id: number;
	level: ScriptLogLevel;
	text: string;
	at: number;
}

export interface ScriptRunHandle {
	done: Promise<void>;
	stop: () => void;
}

let logCounter = 0;

export function createLogEntry({
	level,
	text,
}: {
	level: ScriptLogLevel;
	text: string;
}): ScriptLogEntry {
	logCounter += 1;
	return { id: logCounter, level, text, at: Date.now() };
}

/**
 * Run user code in a fresh sandbox Worker wired to the Editor API bridge.
 * Each run gets its own worker so a stuck script can be terminated cleanly.
 */
export function runScript({
	code,
	bridge,
	onLog,
}: {
	code: string;
	bridge: EditorApiBridge;
	onLog: (entry: ScriptLogEntry) => void;
}): ScriptRunHandle {
	const worker = new Worker(new URL("./script-worker.ts", import.meta.url), {
		type: "module",
		name: "opencut-script",
	});

	let finished = false;
	let resolveDone: () => void = () => {};
	const done = new Promise<void>((resolve) => {
		resolveDone = resolve;
	});

	const unsubscribeEvents = bridge.on("*", (event) => {
		if (finished) return;
		try {
			worker.postMessage(event);
		} catch {
			// worker gone
		}
	});

	const finish = () => {
		if (finished) return;
		finished = true;
		unsubscribeEvents();
		worker.terminate();
		resolveDone();
	};

	worker.addEventListener("message", (event: MessageEvent) => {
		const data = event.data as Record<string, unknown> | null;
		if (!data || typeof data !== "object") return;
		switch (data.type) {
			case "script:ready":
				worker.postMessage({ type: "opencut:ready", apiVersion: EDITOR_API_VERSION });
				worker.postMessage({ type: "script:run", code });
				return;
			case "script:log":
				onLog(
					createLogEntry({
						level: (data.level as ScriptLogLevel) ?? "log",
						text: String(data.text ?? ""),
					}),
				);
				return;
			case "script:done": {
				const duration = Math.round(Number(data.durationMs ?? 0));
				onLog(
					createLogEntry({
						level: "system",
						text:
							data.result !== undefined
								? `Done in ${duration} ms → ${String(data.result)}`
								: `Done in ${duration} ms`,
					}),
				);
				finish();
				return;
			}
			case "script:error":
				onLog(
					createLogEntry({
						level: "error",
						text: String(data.message ?? "Script failed"),
					}),
				);
				finish();
				return;
			case "opencut:request":
				void bridge.handleMessage(data).then((reply) => {
					if (!reply || finished) return;
					worker.postMessage(reply.response, reply.transfer);
				});
				return;
			default:
				return;
		}
	});

	worker.addEventListener("error", (event) => {
		onLog(
			createLogEntry({
				level: "error",
				text: event.message || "The script worker crashed",
			}),
		);
		finish();
	});

	return {
		done,
		stop: () => {
			if (finished) return;
			onLog(createLogEntry({ level: "system", text: "Stopped" }));
			finish();
		},
	};
}

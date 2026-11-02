/// <reference lib="webworker" />
import {
	createEditorApiFacade,
	createRemoteEditorApi,
	workerTransport,
} from "@/api/client";

/**
 * Sandbox for the scripting tab. Receives `{ type: "script:run", code }`,
 * evaluates the code as the body of an async function with `opencut` and a
 * `console` that forwards to the editor, then reports completion.
 */
declare const self: DedicatedWorkerGlobalScope;

type LogLevel = "log" | "info" | "warn" | "error";

const api = createRemoteEditorApi({ transport: workerTransport(self) });

function post(message: unknown) {
	self.postMessage(message);
}

function format(args: unknown[]): string {
	return args
		.map((value) => {
			if (typeof value === "string") return value;
			if (value instanceof Error) return `${value.name}: ${value.message}`;
			try {
				return JSON.stringify(value, null, 2);
			} catch {
				return String(value);
			}
		})
		.join(" ");
}

function makeConsole() {
	const emit =
		(level: LogLevel) =>
		(...args: unknown[]) =>
			post({ type: "script:log", level, text: format(args) });
	return {
		log: emit("log"),
		info: emit("info"),
		warn: emit("warn"),
		error: emit("error"),
		debug: emit("log"),
		table: emit("log"),
	};
}

const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
	...args: string[]
) => (...values: unknown[]) => Promise<unknown>;

self.addEventListener("message", (event: MessageEvent) => {
	const data = event.data as { type?: string; code?: string } | null;
	if (!data || data.type !== "script:run" || typeof data.code !== "string") {
		return;
	}
	void run(data.code);
});

async function run(code: string) {
	const started = performance.now();
	try {
		const fn = new AsyncFunction("opencut", "console", "sleep", code);
		const result = await fn(createEditorApiFacade(api), makeConsole(), (ms: number) =>
			new Promise<void>((resolve) => setTimeout(resolve, ms)),
		);
		post({
			type: "script:done",
			durationMs: performance.now() - started,
			result: result === undefined ? undefined : format([result]),
		});
	} catch (error) {
		post({
			type: "script:error",
			durationMs: performance.now() - started,
			message: error instanceof Error ? `${error.name}: ${error.message}` : String(error),
			stack: error instanceof Error ? error.stack : undefined,
		});
	}
}

post({ type: "script:ready" });

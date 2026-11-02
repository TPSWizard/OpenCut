import { afterEach, describe, expect, test } from "vitest";
import type { EditorCore } from "@/core";
import { installEditorApiBridge, type EditorApiBridge } from "../bridge";
import {
	createEditorApiFacade,
	createRemoteEditorApi,
	type ApiTransport,
} from "../client";
import { EDITOR_API_VERSION, type ApiMessage } from "../types";

/**
 * A stand-in for EditorCore with no open project. Enough for the bridge to
 * install, answer `app.info`, and reject project-bound calls.
 */
function fakeEditor(): EditorCore {
	const subscribe = () => () => {};
	return {
		project: {
			subscribe,
			getActiveOrNull: () => null,
			getActive: () => {
				throw new Error("no project");
			},
		},
		scenes: { subscribe },
		timeline: { subscribe },
		media: { subscribe },
		playback: { subscribe },
		selection: { subscribe },
		command: { canUndo: () => false, canRedo: () => true },
	} as unknown as EditorCore;
}

/** Connects a remote client straight to a bridge, like a Worker would. */
function loopbackTransport(bridge: EditorApiBridge): ApiTransport {
	const listeners = new Set<(message: ApiMessage) => void>();
	return {
		post: (message) => {
			void bridge.handleMessage(message).then((reply) => {
				if (!reply) return;
				for (const listener of listeners) listener(reply.response);
			});
		},
		subscribe: (listener) => {
			listeners.add(listener);
			queueMicrotask(() =>
				listener({ type: "opencut:ready", apiVersion: EDITOR_API_VERSION }),
			);
			return () => {
				listeners.delete(listener);
			};
		},
	};
}

describe("editor api bridge", () => {
	let bridge: EditorApiBridge | null = null;

	afterEach(() => {
		bridge?.dispose();
		bridge = null;
	});

	test("answers app.info without a project and exposes the method list", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		const info = await bridge.call("app.info");
		expect(info).toEqual({ apiVersion: EDITOR_API_VERSION, ready: false, projectId: null });
		expect(bridge.api.methods).toContain("timeline.addClip");
		expect(bridge.api.methods.length).toBeGreaterThan(30);
	});

	test("rejects project-bound calls with a no_project error over the wire", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		const reply = await bridge.handleMessage({
			type: "opencut:request",
			id: "1",
			method: "timeline.get",
			params: {},
		});
		expect(reply?.response).toMatchObject({
			type: "opencut:response",
			id: "1",
			ok: false,
			error: { code: "no_project" },
		});
	});

	test("reports unknown methods and bad params", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		await expect(bridge.call("nope.missing" as never)).rejects.toMatchObject({
			code: "unknown_method",
		});
		const reply = await bridge.handleMessage({
			type: "opencut:request",
			id: "2",
			method: "app.info",
			params: 42,
		});
		expect(reply?.response.error?.code).toBe("invalid_params");
	});

	test("ignores messages that are not requests", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		expect(await bridge.handleMessage({ type: "something-else" })).toBeNull();
		expect(await bridge.handleMessage(null)).toBeNull();
	});

	test("remote client round-trips through the message protocol", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		const client = createRemoteEditorApi({ transport: loopbackTransport(bridge) });
		await client.ready();
		const facade = createEditorApiFacade(client);
		expect(facade.apiVersion).toBe(EDITOR_API_VERSION);
		await expect(facade.history.state()).resolves.toEqual({ canUndo: false, canRedo: true });
		await expect(facade.project.get()).rejects.toMatchObject({ code: "no_project" });
		client.dispose();
	});

	test("namespaced access on the bridge itself proxies to call", async () => {
		bridge = installEditorApiBridge({ editor: fakeEditor(), target: null });
		await expect(bridge.history.state()).resolves.toEqual({ canUndo: false, canRedo: true });
		expect((bridge as unknown as Record<string, unknown>).notANamespace).toBeUndefined();
	});
});

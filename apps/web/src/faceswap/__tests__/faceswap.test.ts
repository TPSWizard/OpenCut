import { describe, expect, it } from "vitest";
import { runFaceSwap } from "../client";
import { fitWithin } from "../apply";
import { FACE_SWAP_EXPORT_NOTE, usesFaceSwap } from "../disclosure";

/** Answers the worker's face swap calls in order; records them. */
function fakeServer(statuses: { status: string; error?: string }[]) {
	const calls: { url: string; method: string; headers: Record<string, string>; body?: unknown }[] = [];
	let polls = 0;
	const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = String(input);
		const method = init?.method ?? "GET";
		calls.push({ url, method, headers: (init?.headers ?? {}) as Record<string, string>, body: init?.body });
		const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
		if (url.endsWith("/faceswap/jobs") && method === "POST") return json({ id: "job1" }, 201);
		if (/\/faceswap\/jobs\/job1\/(face|target)$/.test(url)) return json({ stored: true });
		if (url.endsWith("/job1/run")) return json({ status: "queued" }, 202);
		if (url.endsWith("/job1/output")) return new Response(new Uint8Array([7, 7]), { headers: { "Content-Type": "video/mp4" } });
		if (url.endsWith("/job1")) return json(statuses[Math.min(polls++, statuses.length - 1)]);
		return json({ error: "unexpected" }, 500);
	}) as typeof fetch;
	return { fetchImpl, calls };
}

describe("face swap client", () => {
	it("uploads, polls and downloads with the passcode on every call", async () => {
		const server = fakeServer([{ status: "queued" }, { status: "processing" }, { status: "done" }]);
		const stages: string[] = [];
		const result = await runFaceSwap({
			kind: "video",
			face: new Blob([new Uint8Array([1])], { type: "image/jpeg" }),
			target: new Blob([new Uint8Array([2])], { type: "video/mp4" }),
			faceIndex: 2,
			quality: "fast",
			passcode: "secret",
			onStage: (stage) => stages.push(stage),
			fetchImpl: server.fetchImpl,
			sleep: async () => {},
		});
		expect(new Uint8Array(await result.arrayBuffer())).toEqual(new Uint8Array([7, 7]));
		expect(stages).toEqual(["uploading", "queued", "processing", "downloading"]);
		expect(server.calls.every((call) => call.headers["X-Share-Key"] === "secret")).toBe(true);
		expect(JSON.parse(String(server.calls[0].body))).toEqual({ kind: "video", faceIndex: 2, quality: "fast", consent: true });
		expect(server.calls[1]).toMatchObject({ method: "PUT", headers: { "Content-Type": "image/jpeg" } });
		expect(server.calls[2]).toMatchObject({ method: "PUT", headers: { "Content-Type": "video/mp4" } });
	});

	it("passes on the server's reason for a failed swap", async () => {
		const server = fakeServer([{ status: "failed", error: "No face detected in target" }]);
		await expect(
			runFaceSwap({
				kind: "image",
				face: new Blob([new Uint8Array([1])], { type: "image/png" }),
				target: new Blob([new Uint8Array([2])], { type: "image/png" }),
				passcode: "p",
				fetchImpl: server.fetchImpl,
				sleep: async () => {},
			}),
		).rejects.toThrow("No face detected in target");
	});

	it("gives up after the time limit", async () => {
		const server = fakeServer([{ status: "processing" }]);
		let clock = 0;
		await expect(
			runFaceSwap({
				kind: "video",
				face: new Blob([new Uint8Array([1])], { type: "image/png" }),
				target: new Blob([new Uint8Array([2])], { type: "video/mp4" }),
				passcode: "p",
				fetchImpl: server.fetchImpl,
				sleep: async () => {
					clock += 60_000;
				},
				now: () => clock,
			}),
		).rejects.toThrow("took too long");
	});
});

describe("face swap helpers", () => {
	it("fits frames within a long edge in even pixels", () => {
		expect(fitWithin({ width: 3840, height: 2160, edge: 1920 })).toEqual({ width: 1920, height: 1080 });
		expect(fitWithin({ width: 1081, height: 1921, edge: 1920 })).toEqual({ width: 1080, height: 1920 });
		expect(fitWithin({ width: 640, height: 361, edge: 1920 })).toEqual({ width: 640, height: 360 });
	});

	it("knows when an export shows a swapped face", () => {
		const assets = [
			{ id: "a", origin: { kind: "generated" as const, tool: "faceswap" } },
			{ id: "b", origin: { kind: "generated" as const, tool: "cutout" } },
			{ id: "c" },
		];
		expect(usesFaceSwap({ assets, mediaIds: ["c", "a"] })).toBe(true);
		expect(usesFaceSwap({ assets, mediaIds: ["b", "c"] })).toBe(false);
		expect(FACE_SWAP_EXPORT_NOTE).toContain("face swap");
	});
});

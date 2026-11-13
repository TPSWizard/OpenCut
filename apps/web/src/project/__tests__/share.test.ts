// @vitest-environment node
// Node's own Blob and fetch, as a browser has them (jsdom's Blob is not one fetch can send).
import { describe, expect, it } from "vitest";
import { bundleUrlFromLink } from "../open-link";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { readProjectBundle } from "../bundle";
import { fetchLinkedBundle } from "../open-link";
import { ShareError, shareBundle } from "../share";

type Call = { url: string; method: string; key: string | null; size?: number };

function fakeServer({ failPart }: { failPart?: number } = {}) {
	const calls: Call[] = [];
	const fetchImpl = (async (input: string, init: RequestInit) => {
		const url = String(input);
		const headers = new Headers(init.headers);
		const call: Call = { url, method: init.method ?? "GET", key: headers.get("X-Share-Key") };
		if (init.body instanceof Blob) call.size = init.body.size;
		calls.push(call);
		if (call.key !== "secret") return new Response(JSON.stringify({ error: "nope" }), { status: 401 });
		if (url.endsWith("/share/start")) {
			return Response.json({ id: "AAAAAAAAAAAAAAAAAAAAAA", uploadId: "u1", partSize: 4, parts: 3 });
		}
		if (url.includes("/share/part")) {
			const part = Number(new URL(url, "http://x").searchParams.get("part"));
			if (part === failPart) return new Response(JSON.stringify({ error: "bad" }), { status: 400 });
			return Response.json({ partNumber: part, etag: `e${part}` });
		}
		if (url.endsWith("/share/complete")) {
			return Response.json({ url: "https://api.example/share/AAAAAAAAAAAAAAAAAAAAAA", bytes: 10, expiresAt: "2026-11-20T00:00:00.000Z" });
		}
		if (url.endsWith("/share/abort")) return Response.json({ aborted: true });
		return new Response("{}", { status: 404 });
	}) as unknown as typeof fetch;
	return { calls, fetchImpl };
}

describe("shareBundle", () => {
	it("uploads in parts and returns an Open in ConzaCut link", async () => {
		const { calls, fetchImpl } = fakeServer();
		const progress: number[] = [];
		const result = await shareBundle({
			blob: new Blob([new Uint8Array(10)]),
			name: "Promo",
			passcode: "secret",
			fetchImpl,
			origin: "https://conzacut.conzahub.com",
			onProgress: (share) => progress.push(share),
		});
		expect(calls.filter((call) => call.url.includes("/share/part")).map((call) => call.size)).toEqual([4, 4, 2]);
		expect(progress).toEqual([1 / 3, 2 / 3, 1]);
		expect(result.bundleUrl).toBe("https://api.example/share/AAAAAAAAAAAAAAAAAAAAAA");
		expect(bundleUrlFromLink(result.link)).toBe(result.bundleUrl);
		expect(result.link.startsWith("https://conzacut.conzahub.com/open?bundle=")).toBe(true);
	});

	it("aborts the upload when a part fails", async () => {
		const { calls, fetchImpl } = fakeServer({ failPart: 2 });
		await expect(shareBundle({ blob: new Blob([new Uint8Array(10)]), name: "P", passcode: "secret", fetchImpl })).rejects.toBeInstanceOf(ShareError);
		expect(calls.at(-1)?.url.endsWith("/share/abort")).toBe(true);
	});

	it("says when the passcode is wrong", async () => {
		const { fetchImpl } = fakeServer();
		await expect(shareBundle({ blob: new Blob([new Uint8Array(10)]), name: "P", passcode: "guess", fetchImpl })).rejects.toThrow("Wrong sharing passcode");
	});
});

/**
 * `OPENCUT_SHARE_E2E_KEY=<passcode> VITE_API_URL=<worker>` shares a starter
 * template's bundle through a real worker (local `wrangler dev` or the
 * deployed one) and opens the link again.
 */
describe.runIf(Boolean(process.env.OPENCUT_SHARE_E2E_KEY))("sharing through a real worker", () => {
	it("shares a bundle and gets the same bytes back from its link", async () => {
		const bytes = readFileSync(join(__dirname, "../../../public/templates/countdown.opencut.zip"));
		const result = await shareBundle({ blob: new Blob([bytes]), name: "E2E countdown", passcode: process.env.OPENCUT_SHARE_E2E_KEY!, origin: "https://conzacut.conzahub.com" });
		const url = bundleUrlFromLink(result.link);
		expect(url).toBe(result.bundleUrl);
		const back = await fetchLinkedBundle({ url: url! });
		expect(new Uint8Array(await back.arrayBuffer())).toEqual(new Uint8Array(bytes));
		const { project } = await readProjectBundle({ file: back });
		expect(project.metadata.name).toBe("Countdown");
		console.log("E2E share", result.bundleUrl, result.expiresAt);
	});
});

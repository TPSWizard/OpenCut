import { describe, expect, it } from "vitest";
import cases from "../../../../../fixtures/open-links/cases.json";
import { bundleUrlFromLink, fetchLinkedBundle, openLinkFor } from "../open-link";

describe("bundleUrlFromLink", () => {
	it("matches the desktop editor on the shared cases", () => {
		for (const { input, url } of cases.cases) {
			expect(bundleUrlFromLink(input), input).toBe(url);
		}
	});

	it("round-trips an open link it makes", () => {
		const bundleUrl = "https://cdn.example.com/p.zip?sig=1&e=2";
		const link = openLinkFor({ bundleUrl, origin: "https://conzacut.conzahub.com/" });
		expect(link).toBe("https://conzacut.conzahub.com/open?bundle=https%3A%2F%2Fcdn.example.com%2Fp.zip%3Fsig%3D1%26e%3D2");
		expect(bundleUrlFromLink(link)).toBe(bundleUrl);
	});
});

describe("fetchLinkedBundle", () => {
	it("reports progress and refuses bundles over the limit", async () => {
		const body = new Uint8Array(10);
		const fetchImpl = (async () => new Response(body, { headers: { "content-length": "10" } })) as unknown as typeof fetch;
		const seen: number[] = [];
		const blob = await fetchLinkedBundle({ url: "https://x.example/a.zip", fetchImpl, onProgress: ({ received }) => seen.push(received) });
		expect(blob.size).toBe(10);
		expect(seen.at(-1)).toBe(10);
		await expect(fetchLinkedBundle({ url: "https://x.example/a.zip", fetchImpl, maxBytes: 5 })).rejects.toThrow(/too large/);
	});

	it("explains a blocked download", async () => {
		const fetchImpl = (async () => {
			throw new TypeError("Failed to fetch");
		}) as unknown as typeof fetch;
		await expect(fetchLinkedBundle({ url: "https://x.example/a.zip", fetchImpl })).rejects.toThrow(/CORS/);
	});
});

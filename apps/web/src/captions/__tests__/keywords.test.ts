import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { mediaTimeFromSeconds } from "@/wasm";
import { STOP_WORDS, keywordBudget, keywordIndices, pickKeywordIndices, withKeywords } from "../keywords";

const root = resolve(__dirname, "../../../../..");

describe("caption keywords", () => {
	it("picks the shared cases like the desktop", () => {
		const { cases } = JSON.parse(readFileSync(resolve(root, "fixtures/caption-keywords/cases.json"), "utf8")) as {
			cases: { words: string[]; keywords: number[] }[];
		};
		expect(cases.length).toBeGreaterThan(5);
		for (const { words, keywords } of cases) expect(pickKeywordIndices({ words }), words.join(" ")).toEqual(keywords);
	});

	it("keeps the same stop words as the Rust module", () => {
		const rust = readFileSync(resolve(root, "crates/editor/src/caption_keywords.rs"), "utf8");
		const list = rust.slice(rust.indexOf("pub const STOP_WORDS"), rust.indexOf("];", rust.indexOf("pub const STOP_WORDS")));
		expect([...list.matchAll(/"([a-z']+)"/g)].map((match) => match[1])).toEqual([...STOP_WORDS]);
	});

	it("budgets one to three keywords by length", () => {
		expect([1, 2, 6, 9, 12, 15, 40].map(keywordBudget)).toEqual([0, 1, 1, 2, 2, 3, 3]);
	});

	it("sets and reads the flags", () => {
		const at = (seconds: number) => mediaTimeFromSeconds({ seconds });
		const words = [
			{ text: "a", start: at(0), end: at(1), keyword: true },
			{ text: "b", start: at(1), end: at(2) },
		];
		const flagged = withKeywords({ words, indices: [1] });
		expect(flagged[0]).not.toHaveProperty("keyword");
		expect(flagged[1].keyword).toBe(true);
		expect(keywordIndices({ words: flagged })).toEqual([1]);
		expect(keywordIndices({ words: undefined })).toEqual([]);
	});
});

describe("Claude keywords", () => {
	it("sends the words with the passcode and keeps only valid picks", async () => {
		const { fetchClaudeKeywords } = await import("../keyword-commands");
		const seen: { url: string; init: RequestInit }[] = [];
		const fetchImpl = (async (url: string, init: RequestInit) => {
			seen.push({ url, init });
			return new Response(JSON.stringify({ keywords: [[2, 9, -1], "nope"], model: "claude-opus-5-5" }), { status: 200 });
		}) as unknown as typeof fetch;
		const result = await fetchClaudeKeywords({ captions: [["I", "made", "$5,000"], ["hi"]], passcode: "pass", fetchImpl });
		expect(result).toEqual({ picks: [[2], []], model: "claude-opus-5-5" });
		expect(seen[0].url).toContain("/captions/keywords");
		expect((seen[0].init.headers as Record<string, string>)["X-Share-Key"]).toBe("pass");
		expect(JSON.parse(seen[0].init.body as string)).toEqual({ captions: [{ words: ["I", "made", "$5,000"] }, { words: ["hi"] }] });
		}, 60_000);

	it("reports the server's error", async () => {
		const { fetchClaudeKeywords } = await import("../keyword-commands");
		const fetchImpl = (async () => new Response(JSON.stringify({ error: "Wrong passcode" }), { status: 401 })) as unknown as typeof fetch;
		await expect(fetchClaudeKeywords({ captions: [["a"]], passcode: "x", fetchImpl })).rejects.toThrow("Wrong passcode");
	});
});

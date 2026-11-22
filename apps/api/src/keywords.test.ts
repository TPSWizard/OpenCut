import { describe, expect, it } from "bun:test";
import { KEYWORDS_MODEL, KEYWORDS_SCHEMA, handleKeywords, readKeywords, readKeywordsRequest, userPrompt } from "./keywords";

const env = { ANTHROPIC_API_KEY: "test-key", SHARE_UPLOAD_KEY: "pass" };
const post = (body: unknown, key = "pass") =>
	new Request("https://api.example/captions/keywords", {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": key },
		body: JSON.stringify(body),
	});

function fakeClient(answer: { stop_reason: string; text?: string }) {
	const seen: unknown[] = [];
	const client = {
		beta: {
			messages: {
				stream: (params: unknown) => {
					seen.push(params);
					return {
						finalMessage: async () => ({
							model: KEYWORDS_MODEL,
							stop_reason: answer.stop_reason,
							content: answer.text === undefined ? [] : [{ type: "thinking", thinking: "…" }, { type: "text", text: answer.text }],
						}),
					};
				},
			},
		},
	};
	return { client: client as never, seen };
}

const captions = [{ words: ["I", "made", "$5,000"] }, { words: ["in", "one", "single", "week"] }];

describe("caption keywords from Claude", () => {
	it("checks the captions", () => {
		expect(readKeywordsRequest({ captions: [{ words: ["  Hi ", "there"] }] })).toEqual({ captions: [["Hi", "there"]] });
		expect(typeof readKeywordsRequest({ captions: [] })).toBe("string");
		expect(typeof readKeywordsRequest({ captions: [{ words: [] }] })).toBe("string");
		expect(typeof readKeywordsRequest({ captions: [{ words: [3] }] })).toBe("string");
		expect(typeof readKeywordsRequest({ captions: [{ words: Array.from({ length: 81 }, () => "w") }] })).toBe("string");
		expect(userPrompt({ captions: [["I", "made"], ["it"]] })).toBe("0: 0=I 1=made\n1: 0=it");
	});

	it("keeps picks in range, one list per caption, at most three", () => {
		const request = { captions: [["a", "b", "c", "d", "e"], ["x", "y"], ["z"]] };
		const text = JSON.stringify({
			captions: [
				{ caption: 0, words: [4, 1, 1, 9, 0, 2] },
				{ caption: 1, words: [-1, 1] },
				{ caption: 1, words: [0] },
				{ caption: 7, words: [0] },
			],
		});
		expect(readKeywords(text, request)).toEqual([[0, 1, 4], [1], []]);
		expect(readKeywords("nope", request)).toBeNull();
		expect(readKeywords("{}", request)).toBeNull();
	});

	it("asks Claude with the schema, adaptive thinking, low effort and fallbacks", async () => {
		const { client, seen } = fakeClient({ stop_reason: "end_turn", text: '{"captions":[{"caption":0,"words":[2]},{"caption":1,"words":[3]}]}' });
		const response = await handleKeywords(post({ captions }), env, {}, client);
		expect(response.status).toBe(200);
		expect(await response.json()).toEqual({ keywords: [[2], [3]], model: KEYWORDS_MODEL });
		const params = seen[0] as Record<string, unknown>;
		expect(params.model).toBe("claude-opus-5-5");
		expect(params.thinking).toEqual({ type: "adaptive" });
		expect(params.fallbacks).toBe("default");
		expect(params.output_config).toEqual({ effort: "low", format: { type: "json_schema", schema: KEYWORDS_SCHEMA } });
	});

	it("answers refusals, wrong passcodes and missing setup clearly", async () => {
		const refused = fakeClient({ stop_reason: "refusal" });
		expect((await handleKeywords(post({ captions }), env, {}, refused.client)).status).toBe(422);
		const bad = fakeClient({ stop_reason: "end_turn", text: "{}" });
		expect((await handleKeywords(post({ captions }), env, {}, bad.client)).status).toBe(502);
		const never = fakeClient({ stop_reason: "end_turn", text: "{}" });
		expect((await handleKeywords(post({ captions }, "wrong"), env, {}, never.client)).status).toBe(401);
		expect(never.seen.length).toBe(0);
		expect((await handleKeywords(post({ captions }), { SHARE_UPLOAD_KEY: "pass" }, {}, never.client)).status).toBe(503);
		expect((await handleKeywords(post({ captions: [] }), env, {}, never.client)).status).toBe(400);
	});
});

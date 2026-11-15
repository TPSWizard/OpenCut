import { describe, expect, it } from "bun:test";
import { BROLL_MODEL, QUERIES_SCHEMA, handleBroll, readLines, readQueries, userPrompt } from "./broll";

const env = { ANTHROPIC_API_KEY: "test-key", SHARE_UPLOAD_KEY: "pass" };
const post = (body: unknown, key = "pass") =>
	new Request("https://api.example/broll/plan", {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": key },
		body: JSON.stringify(body),
	});

/** A stand-in for the SDK client: records the request, answers as told. */
function fakeClient(answer: { stop_reason: string; text?: string }) {
	const seen: unknown[] = [];
	const client = {
		beta: {
			messages: {
				create: async (params: unknown) => {
					seen.push(params);
					return {
						model: BROLL_MODEL,
						stop_reason: answer.stop_reason,
						content: answer.text === undefined ? [] : [{ type: "text", text: answer.text }],
					};
				},
			},
		},
	};
	return { client: client as never, seen };
}

describe("B-roll queries from Claude", () => {
	it("checks the lines", () => {
		expect(readLines({ lines: ["A dog runs.", " Rain falls. "] })).toEqual(["A dog runs.", "Rain falls."]);
		expect(typeof readLines({ lines: [] })).toBe("string");
		expect(typeof readLines({ lines: ["ok", 3] })).toBe("string");
		expect(typeof readLines({ lines: Array(81).fill("x") })).toBe("string");
		expect(typeof readLines(null)).toBe("string");
		expect(userPrompt(["A", "B"])).toBe("1. A\n2. B");
	});

	it("reads one tidy query per line", () => {
		expect(readQueries('{"queries":["  dog  running ","rain on window"]}', 2)).toEqual(["dog running", "rain on window"]);
		expect(readQueries('{"queries":["one"]}', 2)).toBeNull();
		expect(readQueries('{"queries":["one",""]}', 2)).toBeNull();
		expect(readQueries("not json", 1)).toBeNull();
	});

	it("asks Claude with the schema, low effort and fallbacks", async () => {
		const { client, seen } = fakeClient({ stop_reason: "end_turn", text: '{"queries":["dog running","rain window"]}' });
		const response = await handleBroll(post({ lines: ["A dog runs.", "Rain falls."] }), env, {}, client);
		expect(response.status).toBe(200);
		expect(await response.json()).toEqual({ queries: ["dog running", "rain window"], model: BROLL_MODEL });
		const params = seen[0] as Record<string, unknown>;
		expect(params.model).toBe("claude-opus-5-5");
		expect(params.fallbacks).toBe("default");
		expect(params.betas).toEqual(["server-side-fallback-2026-07-01"]);
		expect(params.output_config).toEqual({ effort: "low", format: { type: "json_schema", schema: QUERIES_SCHEMA } });
		expect((params.messages as Array<{ content: string }>)[0].content).toBe("1. A dog runs.\n2. Rain falls.");
	});

	it("answers refusals, bad answers, wrong passcodes and missing setup clearly", async () => {
		const refused = fakeClient({ stop_reason: "refusal" });
		expect((await handleBroll(post({ lines: ["x"] }), env, {}, refused.client)).status).toBe(422);
		const short = fakeClient({ stop_reason: "end_turn", text: '{"queries":[]}' });
		expect((await handleBroll(post({ lines: ["x"] }), env, {}, short.client)).status).toBe(502);
		const never = fakeClient({ stop_reason: "end_turn", text: "{}" });
		expect((await handleBroll(post({ lines: ["x"] }, "wrong"), env, {}, never.client)).status).toBe(401);
		expect(never.seen.length).toBe(0);
		expect((await handleBroll(post({ lines: ["x"] }), { SHARE_UPLOAD_KEY: "pass" }, {}, never.client)).status).toBe(503);
		expect((await handleBroll(post({ lines: [] }), env, {}, never.client)).status).toBe(400);
	});
});

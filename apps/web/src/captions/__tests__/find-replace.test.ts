import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { captionEdit, captionFindCount } from "opencut-wasm";
import { describe, expect, it } from "vitest";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/captionedit/cases.json"), "utf8")) as {
	captions: unknown[];
	cases: { name: string; op: string; query?: string; replacement?: string; matches?: unknown[]; edited?: unknown[] }[];
};

describe("caption find and replace through WASM", () => {
	it("edits as the reference (and the desktop)", () => {
		const json = JSON.stringify(fixture.captions);
		for (const item of fixture.cases) {
			if (item.op === "find") {
				expect(captionFindCount(json, item.query ?? ""), item.name).toBe(item.matches?.length);
				continue;
			}
			if (item.op === "removeEmoji") {
				const added = JSON.parse(captionEdit("addEmoji", json, "", "")) as { id: string; words: unknown[] }[];
				const again = JSON.stringify(added.map(({ id, words }) => ({ id, words })));
				expect(JSON.parse(captionEdit("removeEmoji", again, "", "")), item.name).toEqual(item.edited);
				continue;
			}
			expect(JSON.parse(captionEdit(item.op, json, item.query ?? "", item.replacement ?? "")), item.name).toEqual(item.edited);
		}
		expect(() => captionEdit("shout", json, "", "")).toThrow();
	});
});

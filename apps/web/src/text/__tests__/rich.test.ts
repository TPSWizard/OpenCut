import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { charIndex, readRichStyles, restyle, richCharStyles } from "../rich";

const root = resolve(__dirname, "../../../../..");
type Styles = Record<string, unknown>[];
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/rich-text/cases.json"), "utf8")) as {
	charStyles: { name: string; json: string; text?: string; styles?: Styles; valid?: boolean }[];
	rebase: { name: string; stored: unknown; text: string; styles: Styles }[];
	edits: { name: string; stored: unknown; text: string; start: number; end: number; patch: Record<string, unknown> | null; result: { text: string; spans: Styles } }[];
};

const plainToNull = (styles: Styles) => (styles.some((style) => Object.keys(style).length > 0) ? styles : null);

describe("rich text through WASM", () => {
	it.each(cases.charStyles.map((c) => [c.name, c] as const))("styles: %s", (_, c) => {
		const got = richCharStyles(c.json, c.text ?? "");
		if (c.valid === false) expect(got).toBeNull();
		else expect(got).toEqual(plainToNull(c.styles ?? []));
	});

	it.each(cases.rebase.map((c) => [c.name, c] as const))("rebase: %s", (_, c) => {
		expect(richCharStyles(JSON.stringify(c.stored), c.text)).toEqual(plainToNull(c.styles));
	});

	it.each(cases.edits.map((c) => [c.name, c] as const))("edit: %s", (_, c) => {
		const got = JSON.parse(
			restyle({ stored: c.stored ? JSON.stringify(c.stored) : undefined, content: c.text, start: c.start, end: c.end, patch: c.patch }),
		);
		expect(got).toEqual(c.result);
	});

	it("reads a text element's styles and counts selections in characters", () => {
		const stored = restyle({ stored: undefined, content: "a😀 big", start: 3, end: 6, patch: { color: "#F00" } });
		expect(readRichStyles({ content: "a😀 big", richText: stored })?.[4]).toEqual({ color: "#ff0000" });
		expect(readRichStyles({ content: "plain" })).toBeNull();
		expect(charIndex("a😀 big", 4)).toBe(3);
	});
});

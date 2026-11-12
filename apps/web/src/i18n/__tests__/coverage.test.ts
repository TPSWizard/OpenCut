import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { catalogFor, resolveLocale, translate } from "..";

const SRC = join(__dirname, "..", "..");

function sourceFiles(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		if (name === "node_modules" || name === "__tests__") return [];
		if (statSync(path).isDirectory()) return sourceFiles(path);
		return /\.(ts|tsx)$/.test(name) ? [path] : [];
	});
}

/** Every literal passed to `t(...)` in the source. */
function usedKeys(): Map<string, string> {
	const keys = new Map<string, string>();
	const pattern = /\bt\(\s*("(?:[^"\\]|\\.)*"|`(?:[^`\\$]|\\.)*`)/g;
	for (const file of sourceFiles(SRC)) {
		const text = readFileSync(file, "utf-8");
		for (const match of text.matchAll(pattern)) {
			const literal = match[1];
			const key = literal.startsWith("`") ? literal.slice(1, -1) : (JSON.parse(literal) as string);
			if (!keys.has(key)) keys.set(key, relative(SRC, file));
		}
	}
	return keys;
}

describe("interface language", () => {
	it("follows the browser when automatic", () => {
		expect(resolveLocale({ preference: "auto", languages: ["zh-TW", "en"] })).toBe("zh-CN");
		expect(resolveLocale({ preference: "auto", languages: ["fr-FR", "en-GB"] })).toBe("en");
		expect(resolveLocale({ preference: "auto", languages: [] })).toBe("en");
		expect(resolveLocale({ preference: "en", languages: ["zh-CN"] })).toBe("en");
	});

	it("falls back to English and fills placeholders", () => {
		expect(translate({ text: "No such text in any catalogue", locale: "zh-CN" })).toBe("No such text in any catalogue");
		expect(translate({ text: "{count} clips", locale: "en", params: { count: 3 } })).toBe("3 clips");
	});

	it("has a Chinese entry for every string the interface translates", () => {
		const zh = catalogFor("zh-CN");
		const missing = [...usedKeys()].filter(([key]) => !(key in zh)).map(([key, file]) => `${file}: ${key}`);
		expect(missing).toEqual([]);
	});

	it("keeps placeholders intact in every Chinese entry", () => {
		const zh = catalogFor("zh-CN");
		const broken = Object.entries(zh).filter(([key, value]) => {
			const names = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
			return names(key) !== names(value);
		});
		expect(broken).toEqual([]);
	});
});

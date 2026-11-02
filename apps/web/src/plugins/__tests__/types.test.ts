import { describe, expect, test } from "vitest";
import {
	isPluginTabKey,
	parsePluginManifest,
	PluginManifestError,
	pluginIdFromTabKey,
	pluginTabKey,
	resolvePluginUrl,
} from "../types";

describe("plugin manifests", () => {
	test("accepts a panel + entry manifest", () => {
		const manifest = parsePluginManifest({
			id: "com.example.stats",
			name: "Stats",
			version: "1.0.0",
			panel: { title: "Stats", url: "./panel.html" },
			entry: "./worker.js",
			description: "d",
		});
		expect(manifest.panel?.url).toBe("./panel.html");
		expect(manifest.entry).toBe("./worker.js");
	});

	test("requires a panel or an entry", () => {
		expect(() =>
			parsePluginManifest({ id: "a.b", name: "x", version: "1" }),
		).toThrow(PluginManifestError);
	});

	test("rejects bad ids and missing fields", () => {
		expect(() =>
			parsePluginManifest({ id: "has space", name: "x", version: "1", entry: "./e.js" }),
		).toThrow(/manifest.id/);
		expect(() => parsePluginManifest({ id: "ok.id", version: "1", entry: "./e.js" })).toThrow(
			/manifest.name/,
		);
		expect(() => parsePluginManifest(null)).toThrow(/JSON object/);
	});

	test("resolves relative urls against the manifest", () => {
		expect(
			resolvePluginUrl({
				base: "https://example.com/plugins/stats/manifest.json",
				url: "./panel.html",
			}),
		).toBe("https://example.com/plugins/stats/panel.html");
		expect(
			resolvePluginUrl({ base: "https://example.com/m.json", url: "https://cdn.x/y.js" }),
		).toBe("https://cdn.x/y.js");
	});

	test("tab keys round-trip", () => {
		const key = pluginTabKey("com.example.stats");
		expect(isPluginTabKey(key)).toBe(true);
		expect(isPluginTabKey("media")).toBe(false);
		expect(pluginIdFromTabKey(key)).toBe("com.example.stats");
	});
});

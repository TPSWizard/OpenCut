import { describe, expect, it } from "bun:test";
import { MAX_SHARE_BYTES, SHARE_PART_BYTES, bundleFileName, isExpired, isShareId, newShareId, partCount, passcodeMatches, pruneExpired, publicBase } from "./share";

describe("share helpers", () => {
	it("makes unguessable ids it recognises", () => {
		const ids = new Set(Array.from({ length: 50 }, () => newShareId()));
		expect(ids.size).toBe(50);
		for (const id of ids) expect(isShareId(id)).toBe(true);
		expect(isShareId("../x")).toBe(false);
		expect(isShareId("short")).toBe(false);
	});

	it("plans parts within R2's rules", () => {
		expect(partCount(1)).toBe(1);
		expect(partCount(SHARE_PART_BYTES)).toBe(1);
		expect(partCount(SHARE_PART_BYTES + 1)).toBe(2);
		expect(partCount(MAX_SHARE_BYTES)).toBe(64);
		expect(SHARE_PART_BYTES).toBeGreaterThanOrEqual(5 * 1024 * 1024);
	});

	it("checks the passcode", async () => {
		expect(await passcodeMatches("abc", "abc")).toBe(true);
		expect(await passcodeMatches("abd", "abc")).toBe(false);
		expect(await passcodeMatches(null, "abc")).toBe(false);
		expect(await passcodeMatches("", "abc")).toBe(false);
	});

	it("names downloads safely and expires after 7 days", () => {
		expect(bundleFileName('My "promo"/v2')).toBe("My promov2.opencut.zip");
		expect(bundleFileName(undefined)).toBe("project.opencut.zip");
		const now = Date.UTC(2026, 10, 13);
		expect(isExpired(new Date(now - 6 * 86_400_000), now)).toBe(false);
		expect(isExpired(new Date(now - 8 * 86_400_000), now)).toBe(true);
	});

	it("builds share URLs through a gateway's path prefix", () => {
		const at = (prefix?: string) =>
			publicBase(new Request("https://cut.example/share/complete", prefix === undefined ? {} : { headers: { "X-Forwarded-Prefix": prefix } }));
		expect(at()).toBe("https://cut.example");
		expect(at("/api")).toBe("https://cut.example/api");
		expect(at("/tools/api")).toBe("https://cut.example/tools/api");
		for (const bad of ["api", "/api/", "/../x", "//evil.example", "/a b", "/a?b"]) expect(at(bad)).toBe("https://cut.example");
	});

	it("prunes expired shares across pages", async () => {
		const now = Date.UTC(2026, 10, 14);
		const day = 86_400_000;
		const stored = new Map<string, Date>();
		for (let index = 0; index < 1500; index++) stored.set(`shares/${index}.opencut.zip`, new Date(now - (index % 3 === 0 ? 8 : 1) * day));
		stored.set("other/keep.zip", new Date(now - 30 * day));
		const bucket = {
			async list(options?: R2ListOptions) {
				const keys = [...stored.keys()].filter((key) => key.startsWith(options?.prefix ?? "")).sort();
				// Like R2's, the cursor is a position in key order, so deleting
				// listed keys does not skip later ones.
				const after = keys.filter((key) => options?.cursor === undefined || key > options.cursor);
				const slice = after.slice(0, options?.limit ?? 1000);
				const truncated = slice.length < after.length;
				return {
					objects: slice.map((key) => ({ key, uploaded: stored.get(key) as Date })),
					truncated,
					cursor: truncated ? slice[slice.length - 1] : undefined,
					delimitedPrefixes: [],
				} as unknown as R2Objects;
			},
			async delete(keys: string | string[]) {
				for (const key of Array.isArray(keys) ? keys : [keys]) stored.delete(key);
			},
		};
		expect(await pruneExpired(bucket, now)).toBe(500);
		expect(stored.size).toBe(1001);
		expect(stored.has("other/keep.zip")).toBe(true);
		expect(await pruneExpired(bucket, now)).toBe(0);
	});
});

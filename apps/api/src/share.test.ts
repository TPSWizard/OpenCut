import { describe, expect, it } from "bun:test";
import { MAX_SHARE_BYTES, SHARE_PART_BYTES, bundleFileName, isExpired, isShareId, newShareId, partCount, passcodeMatches } from "./share";

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
});

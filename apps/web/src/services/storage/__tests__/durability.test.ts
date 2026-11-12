import { describe, expect, test } from "vitest";
import { assessDurability, describeBackupAge, detectBrowserFamily, needsBackup } from "../durability";

const DAY = 24 * 60 * 60 * 1000;

describe("browser family", () => {
	test("tells Safari from the Chromium browsers that also say Safari", () => {
		const safari =
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";
		const chrome =
			"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
		const edge = `${chrome} Edg/130.0.0.0`;
		const iosChrome =
			"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/130.0 Mobile/15E148 Safari/604.1";
		const firefox = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";
		expect(detectBrowserFamily({ userAgent: safari, vendor: "Apple Computer, Inc." })).toBe("safari");
		expect(detectBrowserFamily({ userAgent: chrome, vendor: "Google Inc." })).toBe("chromium");
		expect(detectBrowserFamily({ userAgent: edge })).toBe("chromium");
		expect(detectBrowserFamily({ userAgent: iosChrome, vendor: "Apple Computer, Inc." })).toBe("chromium");
		expect(detectBrowserFamily({ userAgent: firefox })).toBe("firefox");
		expect(detectBrowserFamily({ userAgent: "curl/8.0" })).toBe("other");
	});
});

describe("durability", () => {
	test("persisted storage in Chromium is protected", () => {
		const result = assessDurability({ persisted: true, browser: "chromium", standalone: false });
		expect(result.level).toBe("protected");
		expect(result.reasons).toEqual([]);
	});

	test("unpersisted storage is at risk and the advice asks for persistence first", () => {
		const result = assessDurability({ persisted: false, browser: "firefox", standalone: false });
		expect(result.level).toBe("at-risk");
		expect(result.advice[0]).toMatch(/persistent storage/);
	});

	test("Safari in a tab is at risk even when persisted, and warns near the seventh day", () => {
		const result = assessDurability({ persisted: true, browser: "safari", standalone: false, sinceLastVisitMs: 6 * DAY });
		expect(result.level).toBe("at-risk");
		expect(result.sevenDayRule).toBe(true);
		expect(result.reasons.join(" ")).toMatch(/7 days/);
		expect(result.reasons.join(" ")).toMatch(/6 days ago/);
		expect(result.advice.join(" ")).toMatch(/Home Screen/);
	});

	test("Safari as an installed app is not under the seven-day rule", () => {
		const result = assessDurability({ persisted: true, browser: "safari", standalone: true });
		expect(result.level).toBe("protected");
		expect(result.sevenDayRule).toBe(false);
	});

	test("an unknown answer is reported as unknown", () => {
		expect(assessDurability({ persisted: null, browser: "other", standalone: false }).level).toBe("unknown");
	});
});

describe("backups", () => {
	test("projects changed after their backup need another", () => {
		expect(needsBackup({ updatedAt: 10 })).toBe(true);
		expect(needsBackup({ updatedAt: 10, lastBackupAt: 20 })).toBe(false);
		expect(needsBackup({ updatedAt: 30, lastBackupAt: 20 })).toBe(true);
	});

	test("ages read naturally", () => {
		const now = 100 * DAY;
		expect(describeBackupAge({ now })).toBe("never");
		expect(describeBackupAge({ lastBackupAt: now - 1000, now })).toBe("today");
		expect(describeBackupAge({ lastBackupAt: now - DAY - 1000, now })).toBe("yesterday");
		expect(describeBackupAge({ lastBackupAt: now - 9 * DAY, now })).toBe("9 days ago");
	});
});

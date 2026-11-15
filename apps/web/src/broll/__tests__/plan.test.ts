import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { planShots, queryFor } from "@/broll/plan";

const cases = JSON.parse(readFileSync(join(__dirname, "../../../../../fixtures/broll/cases.json"), "utf8"));

describe("script to B-roll plan (fixtures/broll, shared with the desktop)", () => {
	it("picks the same queries", () => {
		for (const { text, query } of cases.queries) expect(queryFor({ text }), text).toBe(query);
	});

	it("makes the same shot lists", () => {
		for (const plan of cases.plans) {
			expect(planShots({ script: plan.script, wordsPerSecond: plan.wordsPerSecond })).toEqual(plan.shots);
		}
	});
});

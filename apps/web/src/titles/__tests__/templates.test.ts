import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { creditRollAt, getTextTemplateLibrary, placeTemplate, readCreditRoll, textTemplateElements } from "../templates";
import type { MediaTime } from "@/wasm";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/text-templates/cases.json"), "utf8")) as {
	rolls: Array<{
		name: string;
		direction: string;
		speed: number;
		local: number;
		clip: number;
		canvas: [number, number];
		rect: [number, number, number, number];
		expected: number;
	}>;
	placements: Array<{
		template: string;
		canvas: [number, number];
		items: Array<{ name: string; start: number; duration: number; fontSize: number; x: number; y: number }>;
	}>;
};

describe("titles (row 98) through WASM", () => {
	it("rolls as the reference does", () => {
		for (const roll of fixture.rolls) {
			const placed = creditRollAt({
				roll: readCreditRoll({ roll: roll.direction, rollSpeed: roll.speed })!,
				localSeconds: roll.local,
				clipSeconds: roll.clip,
				canvasWidth: roll.canvas[0],
				canvasHeight: roll.canvas[1],
				box: { left: roll.rect[0], top: roll.rect[1], width: roll.rect[2], height: roll.rect[3] },
			});
			expect(placed?.axis, roll.name).toBe(roll.direction === "up" ? "y" : "x");
			expect(placed!.position, roll.name).toBeCloseTo(roll.expected, 9);
		}
		expect(readCreditRoll({ roll: "none" })).toBeNull();
	});

	it("places every template as the reference does", () => {
		const { templates } = getTextTemplateLibrary();
		expect(templates.length).toBeGreaterThanOrEqual(9);
		for (const placement of fixture.placements) {
			const placed = placeTemplate({ id: placement.template, width: placement.canvas[0], height: placement.canvas[1] })!;
			expect(placed.map((item) => item.name)).toEqual(placement.items.map((item) => item.name));
			placed.forEach((item, index) => {
				const want = placement.items[index];
				expect(item.start).toBeCloseTo(want.start, 9);
				expect(item.duration).toBeCloseTo(want.duration, 9);
				expect(item.params.fontSize).toBeCloseTo(want.fontSize, 9);
				expect(item.params["transform.positionX"]).toBeCloseTo(want.x, 9);
				expect(item.params["transform.positionY"]).toBeCloseTo(want.y, 9);
			});
		}
		expect(placeTemplate({ id: "nope", width: 1920, height: 1080 })).toBeNull();
	});

	it("groups a template's clips and starts them at their offsets", () => {
		const elements = textTemplateElements({ id: "title-subtitle", width: 1920, height: 1080, startTime: 0 as MediaTime })!;
		expect(elements).toHaveLength(2);
		const [title, subtitle] = elements as Array<{ groupId?: string; startTime: number; params: Record<string, unknown> }>;
		expect(title.groupId).toBeTruthy();
		expect(subtitle.groupId).toBe(title.groupId);
		expect(subtitle.startTime).toBeGreaterThan(title.startTime);
		expect(title.params.content).toBe("Your title");
		expect(title.params.animIn).toBe("fade");
	});
});

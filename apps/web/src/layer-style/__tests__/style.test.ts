import { describe, expect, it } from "vitest";
import { layerStyleDescriptor, parseHexColor, readLayerStyle } from "../style";

describe("layer style", () => {
	it("parses hex colours", () => {
		expect(parseHexColor("#ff0000")).toEqual([1, 0, 0, 1]);
		expect(parseHexColor("#0f08")).toEqual([0, 1, 0, 136 / 255]);
		expect(parseHexColor("red")).toBeNull();
		expect(parseHexColor(4)).toBeNull();
	});

	it("reads nothing from plain params", () => {
		expect(readLayerStyle({})).toBeNull();
		expect(readLayerStyle({ cornerRadius: 0, "outline.width": 0, "shadow.enabled": false })).toBeNull();
	});

	it("reads corners, outline and shadow like the desktop", () => {
		const style = readLayerStyle({
			cornerRadius: 50,
			"outline.width": 80,
			"outline.color": "#ff0000",
			"shadow.enabled": true,
			"shadow.opacity": 0.5,
			"shadow.offsetY": -4,
		});
		expect(style).toEqual({
			cornerRadius: 50,
			outline: { width: 50, color: [1, 0, 0, 1] },
			shadow: { color: [0, 0, 0, 0.5], offsetX: 8, offsetY: -4, blur: 16 },
		});
		expect(readLayerStyle({ cornerRadius: 50 }, { isText: true })).toBeNull();
		expect(layerStyleDescriptor({ style: style!, width: 400, height: 200, scale: 0.5 })).toEqual({
			cornerRadius: 50,
			outline: { width: 25, color: [1, 0, 0, 1] },
			shadow: { color: [0, 0, 0, 0.5], offsetX: 4, offsetY: -2, blur: 8 },
		});
	});
});

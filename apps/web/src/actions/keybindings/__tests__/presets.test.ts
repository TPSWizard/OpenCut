import { describe, expect, it } from "vitest";
import { getDefaultShortcuts } from "@/actions/definitions";
import { KEYBINDING_PRESETS, findPreset, presetShortcuts } from "../presets";

describe("keybinding presets", () => {
	it("keeps the defaults for the editor's own preset", () => {
		const own = presetShortcuts({ preset: KEYBINDING_PRESETS[0] });
		expect([...own]).toEqual([...getDefaultShortcuts()]);
		expect(own.get("ctrl+b")).toBe("split");
		expect(own.get("m")).toBe("toggle-bookmark");
		expect(own.get("i")).toBe("mark-in");
		expect(own.get("shift+delete")).toBe("ripple-delete");
		expect(own.get("down")).toBe("next-edit");
	});

	it("replaces only the overridden actions' keys", () => {
		const premiere = presetShortcuts({ preset: findPreset({ id: "premiere" })! });
		expect(premiere.get("ctrl+k")).toBe("split");
		expect(premiere.get("s")).toBe("toggle-snapping");
		expect(premiere.get("n")).toBeUndefined();
		expect(premiere.get("space")).toBe("toggle-play");
		expect(premiere.get("enter")).toBeUndefined();

		const finalCut = presetShortcuts({ preset: findPreset({ id: "final-cut" })! });
		expect(finalCut.get("delete")).toBe("ripple-delete");
		expect(finalCut.get("shift+delete")).toBe("delete-selected");
	});

	it("gives every preset a map without two actions on one key", () => {
		for (const preset of KEYBINDING_PRESETS) {
			const map = presetShortcuts({ preset });
			expect(map.size).toBeGreaterThan(20);
		}
	});
});

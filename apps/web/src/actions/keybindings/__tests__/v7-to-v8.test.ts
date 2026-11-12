import { describe, expect, it } from "vitest";
import { CURRENT_VERSION, runMigrations } from "../migrations";

describe("v7 to v8", () => {
	it("adds the new default keys where they are free and keeps the user's own", () => {
		const migrated = runMigrations({
			state: { keybindings: { s: "split", i: "toggle-snapping", space: "toggle-play" }, isCustomized: true },
			fromVersion: 7,
		}) as { keybindings: Record<string, string>; isCustomized: boolean };
		expect(CURRENT_VERSION).toBe(8);
		expect(migrated.keybindings.i).toBe("toggle-snapping");
		expect(migrated.keybindings.o).toBe("mark-out");
		expect(migrated.keybindings["ctrl+b"]).toBe("split");
		expect(migrated.keybindings["shift+delete"]).toBe("ripple-delete");
		expect(migrated.keybindings.down).toBe("next-edit");
		expect(migrated.keybindings.m).toBe("toggle-bookmark");
		expect(migrated.isCustomized).toBe(true);
	});
});

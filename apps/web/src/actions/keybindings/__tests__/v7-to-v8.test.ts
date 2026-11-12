import { describe, expect, it } from "vitest";
import { CURRENT_VERSION, runMigrations } from "../migrations";

describe("v7 to v8", () => {
	it("adds the new default keys where they are free and keeps the user's own", () => {
		const migrated = runMigrations({
			state: { keybindings: { s: "split", i: "toggle-snapping", space: "toggle-play" }, isCustomized: true },
			fromVersion: 7,
		}) as { keybindings: Record<string, string>; isCustomized: boolean };
		expect(CURRENT_VERSION).toBe(10);
		expect(migrated.keybindings.i).toBe("toggle-snapping");
		expect(migrated.keybindings.o).toBe("mark-out");
		expect(migrated.keybindings["ctrl+b"]).toBe("split");
		expect(migrated.keybindings["shift+delete"]).toBe("ripple-delete");
		expect(migrated.keybindings.down).toBe("next-edit");
		expect(migrated.keybindings.m).toBe("toggle-bookmark");
		expect(migrated.isCustomized).toBe(true);
	});
});

describe("v8 to v9", () => {
	it("adds select range where Ctrl+Shift+A is free", () => {
		const free = runMigrations({ state: { keybindings: { s: "split" }, isCustomized: true }, fromVersion: 8 }) as {
			keybindings: Record<string, string>;
		};
		expect(free.keybindings["ctrl+shift+a"]).toBe("select-range");
		const taken = runMigrations({
			state: { keybindings: { "ctrl+shift+a": "split" }, isCustomized: true },
			fromVersion: 8,
		}) as { keybindings: Record<string, string> };
		expect(taken.keybindings["ctrl+shift+a"]).toBe("split");
	});
});

describe("v9 to v10", () => {
	it("adds the group and compound keys where they are free", () => {
		const migrated = runMigrations({
			state: { keybindings: { "alt+g": "split" }, isCustomized: true },
			fromVersion: 9,
		}) as { keybindings: Record<string, string> };
		expect(migrated.keybindings["ctrl+g"]).toBe("group-selected");
		expect(migrated.keybindings["ctrl+shift+g"]).toBe("ungroup-selected");
		expect(migrated.keybindings["alt+g"]).toBe("split");
		expect(migrated.keybindings["alt+shift+g"]).toBe("break-apart-compound");
	});
});

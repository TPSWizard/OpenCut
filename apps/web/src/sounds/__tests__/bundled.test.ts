import { describe, expect, it } from "vitest";
import {
	BUNDLED_SOUND_ID_BASE,
	isBundledSoundId,
	searchBundledSounds,
	toSoundEffects,
} from "../bundled";

const manifest = {
	packs: [
		{ id: "kenney-ui", name: "Kenney UI Audio", author: "Kenney", license: "CC0-1.0", url: "https://kenney.nl/assets/ui-audio" },
		{ id: "retro", name: "512 Retro Sounds", author: "Juhani Junkala", license: "CC0-1.0", url: "https://opengameart.org/content/512-sound-effects-8-bit-style" },
	],
	sounds: [
		{ pack: "kenney-ui", name: "click 1", file: "kenney-ui/click-1.ogg", duration: 0.12, tags: ["click"] },
		{ pack: "retro", name: "coin single1", file: "retro/sfx-coin-single1.ogg", duration: 0.3, tags: ["general sounds", "coins"] },
		{ pack: "ghost", name: "orphan", file: "x.ogg", duration: 1, tags: [] },
	],
};

describe("toSoundEffects", () => {
	it("maps manifest entries to panel sounds with stable ids and pack credit", () => {
		const sounds = toSoundEffects({ manifest });
		expect(sounds).toHaveLength(3);
		expect(sounds[0]).toMatchObject({
			id: BUNDLED_SOUND_ID_BASE,
			name: "click 1",
			previewUrl: "/sounds/kenney-ui/click-1.ogg",
			username: "Kenney",
			license: "CC0-1.0",
			tags: ["click", "kenney-ui"],
			duration: 0.12,
		});
		expect(sounds[1].id).toBe(BUNDLED_SOUND_ID_BASE + 1);
		expect(sounds[2].username).toBe("");
		expect(isBundledSoundId({ id: sounds[1].id })).toBe(true);
		expect(isBundledSoundId({ id: 12345 })).toBe(false);
	});
});

describe("searchBundledSounds", () => {
	it("matches every word against name, tags and author", () => {
		const sounds = toSoundEffects({ manifest });
		expect(searchBundledSounds({ sounds, query: "coin" }).map((s) => s.name)).toEqual(["coin single1"]);
		expect(searchBundledSounds({ sounds, query: "kenney click" }).map((s) => s.name)).toEqual(["click 1"]);
		expect(searchBundledSounds({ sounds, query: "" })).toHaveLength(3);
		expect(searchBundledSounds({ sounds, query: "laser" })).toEqual([]);
	});
});

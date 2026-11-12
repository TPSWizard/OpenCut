import { describe, expect, it } from "vitest";
import { buildEmojiCollection, searchEmojiNames, withSkinTone } from "../providers/emoji";
import {
	POPULAR_BRANDS,
	brandColorHex,
	buildBrandUrl,
	loadBrandData,
	parseBrandStickerId,
	searchBrands,
	logosProvider,
} from "../providers/logos";

const categories = {
	"People & Body": ["waving-hand", "waving-hand-dark-skin-tone", "waving-hand-medium-skin-tone", "person"],
	"Smileys & Emotion": ["grinning-face", "grinning-face-with-big-eyes", "red-heart"],
};

describe("emoji pack", () => {
	it("hides skin-toned variants and applies the chosen tone where it exists", () => {
		const collection = buildEmojiCollection({ categories });
		expect(collection.categories["People & Body"]).toEqual(["waving-hand", "person"]);
		expect(withSkinTone({ name: "waving-hand", tone: "dark", names: collection.names })).toBe("waving-hand-dark-skin-tone");
		expect(withSkinTone({ name: "person", tone: "dark", names: collection.names })).toBe("person");
		expect(withSkinTone({ name: "waving-hand", tone: "default", names: collection.names })).toBe("waving-hand");
	});

	it("searches by every word, shortest names first", () => {
		const collection = buildEmojiCollection({ categories });
		expect(searchEmojiNames({ collection, query: "grinning", limit: 10 })).toEqual(["grinning-face", "grinning-face-with-big-eyes"]);
		expect(searchEmojiNames({ collection, query: "big face", limit: 10 })).toEqual(["grinning-face-with-big-eyes"]);
		expect(searchEmojiNames({ collection, query: "  ", limit: 10 })).toEqual([]);
	});
});

describe("brand pack", () => {
	it("ships permissively licensed brands with colours, and popular picks that exist", async () => {
		const data = await loadBrandData();
		expect(data.license).toBe("CC0-1.0");
		expect(data.source).toMatch(/^simple-icons@/);
		expect(data.brands.length).toBeGreaterThan(3000);
		const licences = new Set(data.brands.map((brand) => brand[4]));
		for (const banned of ["CC-BY-SA-4.0", "GPL-3.0-only", "custom", "CC-BY-NC-4.0"]) expect(licences.has(banned)).toBe(false);
		const slugs = new Set(data.brands.map((brand) => brand[0]));
		expect(POPULAR_BRANDS.filter((slug) => slugs.has(slug)).length).toBeGreaterThan(40);
		expect(data.brands.every((brand) => /^[0-9A-F]{6}$/.test(brand[2]))).toBe(true);
	});

	it("searches titles and slugs, exact matches first", async () => {
		const { brands } = await loadBrandData();
		expect(searchBrands({ brands, query: "YouTube", limit: 3 })[0][0]).toBe("youtube");
		expect(searchBrands({ brands, query: "spot", limit: 5 }).map((brand) => brand[0])).toContain("spotify");
	});

	it("colours logos and round-trips sticker ids", async () => {
		expect(brandColorHex({ hex: "1db954", mode: "brand" })).toBe("1DB954");
		expect(brandColorHex({ hex: "1db954", mode: "white" })).toBe("FFFFFF");
		expect(buildBrandUrl({ slug: "spotify", hex: "1DB954", width: 64, height: 64 })).toBe(
			"https://api.iconify.design/simple-icons/spotify.svg?width=64&height=64&color=%231DB954",
		);
		expect(parseBrandStickerId({ stickerId: "logos:spotify:1db954" })).toEqual({ slug: "spotify", hex: "1DB954" });
		expect(() => parseBrandStickerId({ stickerId: "logos:spotify" })).toThrow();
		const found = await logosProvider.search({ query: "spotify", options: { limit: 1 } });
		expect(found.items[0].id).toMatch(/^logos:spotify:[0-9A-F]{6}$/);
		expect(found.items[0].metadata.trademark).toMatch(/trademarks/);
	});
});

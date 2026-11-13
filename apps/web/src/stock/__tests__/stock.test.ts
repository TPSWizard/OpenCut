import { describe, expect, test } from "vitest";
import { creditOf, openverseLicenseLabel, parseOpenverse } from "../stock";

// The same data as `crates/editor/src/stock.rs` tests (shaped like a real
// Openverse response, 2026-11-12).
const fixture = {
	result_count: 2,
	results: [
		{
			id: "49dec960-1cc7-4738-afc6-793315cd51e9",
			title: "gloomy forest",
			url: "https://live.staticflickr.com/8043/8394379683_a49795d0e3_b.jpg",
			thumbnail: "https://api.openverse.org/v1/images/49dec960-1cc7-4738-afc6-793315cd51e9/thumb/",
			creator: "gorchakov.artem",
			creator_url: "https://www.flickr.com/photos/56003265@N05",
			license: "by",
			license_version: "2.0",
			license_url: "https://creativecommons.org/licenses/by/2.0/",
			foreign_landing_url: "https://www.flickr.com/photos/56003265@N05/8394379683",
			width: 1024,
			height: 640,
			attribution:
				'"gloomy forest" by gorchakov.artem is licensed under CC BY 2.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/2.0/.',
		},
		{ id: "", url: "https://example.org/broken.jpg" },
		{
			id: "0f6d0a54-6b0a-4a41-9d6f-3a1fb2a5d1c0",
			title: "",
			url: "https://upload.wikimedia.org/x.jpg",
			thumbnail: "",
			creator: null,
			license: "cc0",
			license_version: "1.0",
			license_url: "https://creativecommons.org/publicdomain/zero/1.0/",
			foreign_landing_url: "https://commons.wikimedia.org/wiki/File:X.jpg",
			width: 800,
			height: 600,
		},
	],
};

describe("stock", () => {
	test("Openverse results become stock items", () => {
		const items = parseOpenverse({ json: fixture });
		expect(items).toHaveLength(2);
		expect(items[0].license).toBe("CC BY 2.0");
		expect(items[0].fileUrl).toBe("https://live.staticflickr.com/8043/8394379683_a49795d0e3_b.jpg");
		expect(items[0].attribution.startsWith('"gloomy forest" by gorchakov.artem')).toBe(true);
		expect(items[1]).toMatchObject({ title: "Untitled", creator: "Unknown", license: "CC0 1.0", attribution: '"Untitled" by Unknown' });
		expect(openverseLicenseLabel({ license: "pdm", version: "1.0" })).toBe("Public Domain Mark 1.0");
	});

	test("a credit carries what the desktop's StockCredit has", () => {
		const credit = creditOf({ item: parseOpenverse({ json: fixture })[0] });
		expect(Object.keys(credit).sort()).toEqual(
			["attribution", "creator", "creatorUrl", "id", "license", "licenseUrl", "provider", "sourceUrl", "title"].sort(),
		);
	});
});

import { describe, expect, it } from "vitest";
import {
	buildIconUrl,
	filterCollections,
	parseIconStickerId,
} from "../providers/icons";

describe("filterCollections", () => {
	it("keeps allow-listed licences, flags credit-only ones and drops the rest", () => {
		const kept = filterCollections({
			raw: {
				mdi: { name: "Material Design Icons", total: 7447, license: { title: "Apache 2.0", spdx: "Apache-2.0" }, samples: ["home"] },
				twemoji: { name: "Twitter Emoji", total: 3000, license: { title: "CC BY 4.0", spdx: "CC-BY-4.0" }, palette: true, samples: [] },
				openmoji: { name: "OpenMoji", total: 4000, license: { title: "CC BY-SA 4.0", spdx: "CC-BY-SA-4.0" }, samples: [] },
				gpl: { name: "Some GPL set", total: 10, license: { title: "GPL", spdx: "GPL-3.0" }, samples: [] },
				unlicensed: { name: "No spdx", total: 10, license: { title: "Custom" }, samples: [] },
				tabler: { name: "Tabler Icons", total: 6257, license: { title: "MIT", spdx: "MIT" }, samples: ["camera"] },
			},
		});
		expect(kept.map((c) => c.prefix)).toEqual(["mdi", "tabler", "twemoji"]);
		expect(kept.find((c) => c.prefix === "twemoji")).toMatchObject({ requiresCredit: true, palette: true });
		expect(kept.find((c) => c.prefix === "mdi")).toMatchObject({ requiresCredit: false, author: { name: "Unknown" } });
	});
});

describe("buildIconUrl", () => {
	it("colours monochrome sets and leaves palette sets alone", () => {
		expect(buildIconUrl({ prefix: "mdi", name: "home", width: 64, height: 64 })).toBe(
			"https://api.iconify.design/mdi/home.svg?width=64&height=64&color=%23ffffff",
		);
		expect(buildIconUrl({ prefix: "twemoji", name: "cat", palette: true })).toBe(
			"https://api.iconify.design/twemoji/cat.svg",
		);
	});
});

describe("parseIconStickerId", () => {
	it("splits provider, prefix and name", () => {
		expect(parseIconStickerId({ stickerId: "icons:mdi:home-outline" })).toEqual({ prefix: "mdi", name: "home-outline" });
		expect(() => parseIconStickerId({ stickerId: "flags:US" })).toThrow();
		expect(() => parseIconStickerId({ stickerId: "icons:home" })).toThrow();
	});
});

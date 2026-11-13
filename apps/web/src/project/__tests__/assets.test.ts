import { describe, expect, it } from "vitest";
import bundle from "../../../../../fixtures/asset-manifest/bundle.json";
import expected from "../../../../../fixtures/asset-manifest/expected.json";
import { buildAssetManifest, fontsourceSlug, licenseRequiresCredit, type AssetLookups } from "../assets";

/** The same lookups as the desktop's test (`crates/editor/src/assets.rs`). */
const lookups: AssetLookups = {
	iconSets: {
		mdi: {
			name: "Material Design Icons",
			license: "Apache-2.0",
			licenseUrl: "https://github.com/Templarian/MaterialDesign/blob/master/LICENSE",
			author: "Pictogrammers",
		},
		cc: { name: "Credit Set", license: "CC-BY-4.0", licenseUrl: null, author: "Someone" },
	},
	brands: { youtube: { title: "YouTube", license: "CC0-1.0", creditRequired: false } },
	fonts: { Inter: "OFL-1.1" },
};

describe("buildAssetManifest", () => {
	it("matches the desktop editor's manifest for the shared fixture", () => {
		expect(buildAssetManifest({ bundle, lookups })).toEqual(expected);
	});

	it("knows nothing it was not told", () => {
		const manifest = buildAssetManifest({ bundle });
		const mdi = manifest.assets.find((entry) => entry.id === "icons:mdi:home");
		expect(mdi?.license).toBeNull();
		expect(manifest.assets.find((entry) => entry.id === "Inter")?.license).toBeNull();
		expect(manifest.credits).toContain("Twemoji by Twitter (jdecked/twemoji), CC BY 4.0");
	});

	it("slugs and credit rules", () => {
		expect(fontsourceSlug("  Open Sans ")).toBe("open-sans");
		expect(fontsourceSlug("M PLUS 1p")).toBe("m-plus-1p");
		expect(licenseRequiresCredit("CC BY 2.0")).toBe(true);
		expect(licenseRequiresCredit("CC0 1.0")).toBe(false);
	});
});

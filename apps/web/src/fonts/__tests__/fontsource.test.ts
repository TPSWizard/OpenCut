import { describe, expect, it } from "vitest";
import {
	buildFontFaceCss,
	describeFontLicense,
	fontsourceId,
	selectFontFaces,
} from "../fontsource";

const variants = {
	"400": {
		normal: { latin: { url: { woff2: "https://cdn/inter@5/latin-400-normal.woff2", ttf: "https://cdn/inter@5/latin-400-normal.ttf" } } },
		italic: { latin: { url: { woff2: "https://cdn/inter@5/latin-400-italic.woff2" } } },
	},
	"700": {
		normal: { latin: { url: { ttf: "https://cdn/inter@5/latin-700-normal.ttf" } } },
	},
	"900": {
		normal: { latin: { url: { woff2: "https://cdn/inter@5/latin-900-normal.woff2" } } },
	},
};

describe("fontsourceId", () => {
	it("slugs the family name the way Fontsource does", () => {
		expect(fontsourceId({ family: "Inter" })).toBe("inter");
		expect(fontsourceId({ family: "Noto Sans JP" })).toBe("noto-sans-jp");
		expect(fontsourceId({ family: "  Press Start 2P " })).toBe("press-start-2p");
	});
});

describe("selectFontFaces", () => {
	it("takes the wanted weights in both styles, preferring woff2", () => {
		const faces = selectFontFaces({
			detail: { variants, defSubset: "latin", weights: [400, 700, 900], styles: ["normal", "italic"] },
		});
		expect(faces).toEqual([
			{ weight: 400, style: "normal", url: "https://cdn/inter@5/latin-400-normal.woff2", format: "woff2" },
			{ weight: 400, style: "italic", url: "https://cdn/inter@5/latin-400-italic.woff2", format: "woff2" },
			{ weight: 700, style: "normal", url: "https://cdn/inter@5/latin-700-normal.ttf", format: "truetype" },
		]);
	});

	it("falls back to the nearest weight the family has", () => {
		const faces = selectFontFaces({
			detail: { variants, defSubset: "latin", weights: [400, 700, 900], styles: ["normal"] },
			weights: [800],
		});
		expect(faces.map((face) => face.weight)).toEqual([700]);
	});
});

describe("buildFontFaceCss", () => {
	it("declares one rule per face and strips quotes from the family", () => {
		const css = buildFontFaceCss({
			family: 'Odd "Name"',
			faces: [{ weight: 400, style: "normal", url: "https://cdn/x.woff2", format: "woff2" }],
		});
		expect(css).toBe(
			'@font-face{font-family:"Odd Name";font-style:normal;font-weight:400;font-display:swap;src:url("https://cdn/x.woff2") format("woff2");}',
		);
	});
});

describe("describeFontLicense", () => {
	it("names the common licences and passes others through", () => {
		expect(describeFontLicense({ license: "OFL-1.1" })).toBe("SIL Open Font License 1.1");
		expect(describeFontLicense({ license: "Apache-2.0" })).toBe("Apache 2.0");
		expect(describeFontLicense({ license: "Custom" })).toBe("Custom");
	});
});

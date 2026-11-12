import { describe, expect, it } from "vitest";
import {
	decodeLutParam,
	encodeLutParam,
	fromHald,
	identityLut,
	lutKey,
	lutTextureRgba,
	parse3dl,
	parseCube,
	resampleLut,
	sampleLut,
} from "../parse";

describe("parseCube", () => {
	it("reads 3D tables with a domain and expands 1D curves", () => {
		const text = 'TITLE "swap"\n# comment\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n';
		const table = parseCube({ text });
		expect(table.size).toBe(2);
		expect(sampleLut({ table, r: 1, g: 0, b: 0 })).toEqual([0, 0, 1]);
		expect(sampleLut({ table, r: 0, g: 0, b: 1 })).toEqual([1, 0, 0]);
		const curve = parseCube({ text: "LUT_1D_SIZE 3\n0 0 0\n0.25 0.5 0.75\n1 1 1\n" });
		expect(curve.size).toBe(3);
		expect(sampleLut({ table: curve, r: 0.5, g: 0.5, b: 0.5 })).toEqual([0.25, 0.5, 0.75]);
		expect(() => parseCube({ text: "LUT_3D_SIZE 2\n0 0 0\n" })).toThrow();
	});
});

describe("parse3dl and fromHald", () => {
	it("land in the same red-fastest layout", () => {
		let text = "0 1023\n";
		for (let r = 0; r < 2; r++) for (let g = 0; g < 2; g++) for (let b = 0; b < 2; b++) text += `${b * 1023} ${g * 1023} ${r * 1023}\n`;
		const table = parse3dl({ text });
		expect(table.size).toBe(2);
		expect(sampleLut({ table, r: 1, g: 0, b: 0 })).toEqual([0, 0, 1]);
		const identity = identityLut({ size: 4 });
		const rgba = new Uint8ClampedArray(64 * 4);
		for (let index = 0; index < 64; index++) {
			rgba[index * 4] = Math.round(identity.rgb[index * 3] * 255);
			rgba[index * 4 + 1] = Math.round(identity.rgb[index * 3 + 1] * 255);
			rgba[index * 4 + 2] = Math.round(identity.rgb[index * 3 + 2] * 255);
			rgba[index * 4 + 3] = 255;
		}
		const parsed = fromHald({ rgba, width: 8, height: 8 });
		expect(parsed.size).toBe(4);
		expect(sampleLut({ table: parsed, r: 0.5, g: 0.25, b: 1 })[0]).toBeCloseTo(0.5, 1);
		expect(() => fromHald({ rgba, width: 4, height: 16 })).toThrow();
	});
});

describe("lut parameter", () => {
	it("round-trips, resamples big tables and keys by content", () => {
		const table = identityLut({ size: 65 });
		const param = encodeLutParam({ name: "Big|LUT", table });
		expect(param.startsWith("Big/LUT|33|")).toBe(true);
		const decoded = decodeLutParam({ param });
		expect(decoded?.name).toBe("Big/LUT");
		expect(decoded?.table.size).toBe(33);
		expect(sampleLut({ table: decoded!.table, r: 0.3, g: 0.6, b: 0.9 })[2]).toBeCloseTo(0.9, 1);
		expect(decodeLutParam({ param: "nope" })).toBeNull();
		expect(lutKey({ param })).not.toBe(lutKey({ param: "x|2|AAAA" }));
		expect(lutKey({ param })).toBe(lutKey({ param }));
		expect(lutTextureRgba({ table: resampleLut({ table, size: 2 }) })).toHaveLength(2 * 2 * 2 * 4);
	});
});

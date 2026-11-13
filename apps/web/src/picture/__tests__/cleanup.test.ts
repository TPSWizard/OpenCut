import { describe, expect, test } from "vitest";
import { cleanPicture, featherAlpha, removeFlatWhite, type Picture } from "../cleanup";

/** The same synthetic drawing as `crates/editor/src/picture.rs` tests. */
function drawing(): Picture {
	const w = 64;
	const h = 48;
	const rgba = new Uint8ClampedArray(w * h * 4).fill(255);
	const set = (x: number, y: number, v: number) => {
		const i = (y * w + x) * 4;
		if (rgba[i] > v) rgba.set([v, v, v], i);
	};
	for (let t = 0; t <= 40; t++) {
		const cx = 10 + t;
		const cy = 6 + Math.floor((t * 3) / 4);
		for (let dy = -2; dy <= 2; dy++) {
			for (let dx = -2; dx <= 2; dx++) {
				const edge = Math.abs(dx) === 2 || Math.abs(dy) === 2;
				set(cx + dx, cy + dy, edge ? 215 : 60);
			}
		}
	}
	for (let y = 34; y < 40; y++) for (let x = 50; x < 56; x++) set(x, y, 0);
	return { width: w, height: h, rgba };
}

describe("picture cleanup", () => {
	test("white paper goes, the ink stays, the rim fades", () => {
		const picture = drawing();
		removeFlatWhite(picture);
		const alpha = (x: number, y: number) => picture.rgba[(y * picture.width + x) * 4 + 3];
		expect(alpha(0, 0)).toBe(0);
		expect(alpha(30, 21)).toBe(255);
		expect(alpha(10, 4)).toBeGreaterThan(0);
		expect(alpha(10, 4)).toBeLessThan(255);
	});

	test("feathering only softens", () => {
		const picture = drawing();
		removeFlatWhite(picture);
		const before = Array.from({ length: picture.width * picture.height }, (_, p) => picture.rgba[p * 4 + 3]);
		featherAlpha(picture, 2);
		const after = Array.from({ length: picture.width * picture.height }, (_, p) => picture.rgba[p * 4 + 3]);
		expect(after.every((a, i) => a <= before[i])).toBe(true);
		expect(after.some((a, i) => a < before[i])).toBe(true);
	});

	test("matches the desktop reference (crates/editor/src/picture.rs)", () => {
		const { picture, path } = cleanPicture(drawing(), {
			removeWhite: true,
			decontaminate: true,
			feather: 1,
			tint: "#ffffff",
			crop: true,
			trace: true,
		});
		let alphaSum = 0;
		for (let i = 3; i < picture.rgba.length; i += 4) alphaSum += picture.rgba[i];
		expect([picture.width, picture.height, alphaSum]).toEqual([52, 40, 58_924]);
		expect(path?.points).toEqual([
			[0.087, 0.113], [0.106, 0.113], [0.163, 0.188], [0.183, 0.188], [0.24, 0.263], [0.26, 0.263],
			[0.317, 0.338], [0.337, 0.338], [0.394, 0.413], [0.413, 0.413], [0.471, 0.488], [0.49, 0.488],
			[0.548, 0.563], [0.567, 0.563], [0.625, 0.638], [0.644, 0.638], [0.702, 0.713], [0.721, 0.713],
			[0.779, 0.788], [0.798, 0.788], [0.837, 0.838], [0.875, 0.838], [0.894, 0.863],
		]);
	});
});

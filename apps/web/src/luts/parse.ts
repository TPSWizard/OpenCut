/**
 * Colour look-up tables: `.cube` and `.3dl` files and HALD CLUT images
 * parsed into a 3D table, resampled to at most 33 points a side, and
 * packed into the `lut` effect parameter as `name|size|base64(RGB8)`
 * (red varies fastest, then green, then blue). The desktop's
 * `crates/editor/src/luts.rs` reads and writes the same format.
 */

export interface LutTable {
	size: number;
	/** `size³` RGB triplets in 0..1, red varying fastest. */
	rgb: Float32Array;
}

export const MAX_LUT_SIZE = 33;

export function identityLut({ size }: { size: number }): LutTable {
	const rgb = new Float32Array(size * size * size * 3);
	const n = Math.max(1, size - 1);
	let index = 0;
	for (let b = 0; b < size; b++) {
		for (let g = 0; g < size; g++) {
			for (let r = 0; r < size; r++) {
				rgb[index++] = r / n;
				rgb[index++] = g / n;
				rgb[index++] = b / n;
			}
		}
	}
	return { size, rgb };
}

function at({ table, r, g, b }: { table: LutTable; r: number; g: number; b: number }): [number, number, number] {
	const index = ((b * table.size + g) * table.size + r) * 3;
	return [table.rgb[index], table.rgb[index + 1], table.rgb[index + 2]];
}

/** Trilinear look-up at `(r, g, b)` in 0..1. */
export function sampleLut({ table, r, g, b }: { table: LutTable; r: number; g: number; b: number }): [number, number, number] {
	const n = table.size - 1;
	const clamp = (v: number) => Math.min(1, Math.max(0, v)) * n;
	const [fr, fg, fb] = [clamp(r), clamp(g), clamp(b)];
	const [r0, g0, b0] = [Math.floor(fr), Math.floor(fg), Math.floor(fb)];
	const [r1, g1, b1] = [Math.min(r0 + 1, n), Math.min(g0 + 1, n), Math.min(b0 + 1, n)];
	const [tr, tg, tb] = [fr - r0, fg - g0, fb - b0];
	const out: [number, number, number] = [0, 0, 0];
	for (let channel = 0; channel < 3; channel++) {
		const c00 = at({ table, r: r0, g: g0, b: b0 })[channel] * (1 - tr) + at({ table, r: r1, g: g0, b: b0 })[channel] * tr;
		const c10 = at({ table, r: r0, g: g1, b: b0 })[channel] * (1 - tr) + at({ table, r: r1, g: g1, b: b0 })[channel] * tr;
		const c01 = at({ table, r: r0, g: g0, b: b1 })[channel] * (1 - tr) + at({ table, r: r1, g: g0, b: b1 })[channel] * tr;
		const c11 = at({ table, r: r0, g: g1, b: b1 })[channel] * (1 - tr) + at({ table, r: r1, g: g1, b: b1 })[channel] * tr;
		out[channel] = (c00 * (1 - tg) + c10 * tg) * (1 - tb) + (c01 * (1 - tg) + c11 * tg) * tb;
	}
	return out;
}

/** The table resampled to `size` points a side. */
export function resampleLut({ table, size }: { table: LutTable; size: number }): LutTable {
	if (size === table.size) return table;
	const rgb = new Float32Array(size * size * size * 3);
	const n = Math.max(1, size - 1);
	let index = 0;
	for (let b = 0; b < size; b++) {
		for (let g = 0; g < size; g++) {
			for (let r = 0; r < size; r++) {
				const [cr, cg, cb] = sampleLut({ table, r: r / n, g: g / n, b: b / n });
				rgb[index++] = cr;
				rgb[index++] = cg;
				rgb[index++] = cb;
			}
		}
	}
	return { size, rgb };
}

/** RGBA8 bytes laid out as the shader's 2D texture: width `size²` (x = r + size·b), height `size` (y = g). */
export function lutTextureRgba({ table }: { table: LutTable }): Uint8Array {
	const { size } = table;
	const out = new Uint8Array(size * size * size * 4).fill(255);
	for (let b = 0; b < size; b++) {
		for (let g = 0; g < size; g++) {
			for (let r = 0; r < size; r++) {
				const [cr, cg, cb] = at({ table, r, g, b });
				const index = (g * size * size + r + size * b) * 4;
				out[index] = Math.round(Math.min(1, Math.max(0, cr)) * 255);
				out[index + 1] = Math.round(Math.min(1, Math.max(0, cg)) * 255);
				out[index + 2] = Math.round(Math.min(1, Math.max(0, cb)) * 255);
			}
		}
	}
	return out;
}

/** Parses an Adobe/Resolve `.cube` file (3D, or 1D expanded to 3D). */
export function parseCube({ text }: { text: string }): LutTable {
	let size3d: number | null = null;
	let size1d: number | null = null;
	const domainMin = [0, 0, 0];
	const domainMax = [1, 1, 1];
	const values: number[][] = [];
	for (const raw of text.split(/\r?\n/)) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const parts = line.split(/\s+/);
		const head = parts[0].toUpperCase();
		if (head === "TITLE") continue;
		if (head === "LUT_3D_SIZE") {
			size3d = Number.parseInt(parts[1], 10);
			continue;
		}
		if (head === "LUT_1D_SIZE") {
			size1d = Number.parseInt(parts[1], 10);
			continue;
		}
		if (head === "DOMAIN_MIN" || head === "DOMAIN_MAX") {
			const triple = parts.slice(1, 4).map(Number);
			if (triple.length !== 3 || triple.some((v) => !Number.isFinite(v))) throw new Error(`bad domain line: ${line}`);
			(head === "DOMAIN_MIN" ? domainMin : domainMax).splice(0, 3, ...triple);
			continue;
		}
		const row = parts.slice(0, 3).map(Number);
		if (row.length !== 3 || row.some((v) => !Number.isFinite(v))) throw new Error(`unexpected line: ${line}`);
		values.push(row);
	}
	const normalise = (row: number[]) => row.map((v, c) => (v - domainMin[c]) / Math.max(1e-6, Math.abs(domainMax[c] - domainMin[c])));
	if (size3d !== null) {
		if (size3d < 2 || values.length !== size3d ** 3) {
			throw new Error(`expected ${size3d ** 3} rows for LUT_3D_SIZE ${size3d}, found ${values.length}`);
		}
		return { size: size3d, rgb: Float32Array.from(values.flatMap(normalise)) };
	}
	if (size1d !== null) {
		if (size1d < 2 || values.length !== size1d) {
			throw new Error(`expected ${size1d} rows for LUT_1D_SIZE ${size1d}, found ${values.length}`);
		}
		const curve = values.map(normalise);
		const n = size1d - 1;
		const lookup = (x: number, channel: number) => {
			const f = Math.min(1, Math.max(0, x)) * n;
			const i = Math.floor(f);
			const j = Math.min(i + 1, n);
			return curve[i][channel] * (1 - (f - i)) + curve[j][channel] * (f - i);
		};
		const size = Math.min(size1d, MAX_LUT_SIZE);
		const m = size - 1;
		const rgb = new Float32Array(size * size * size * 3);
		let index = 0;
		for (let b = 0; b < size; b++) {
			for (let g = 0; g < size; g++) {
				for (let r = 0; r < size; r++) {
					rgb[index++] = lookup(r / m, 0);
					rgb[index++] = lookup(g / m, 1);
					rgb[index++] = lookup(b / m, 2);
				}
			}
		}
		return { size, rgb };
	}
	throw new Error("no LUT_3D_SIZE or LUT_1D_SIZE line");
}

/** Parses an Autodesk `.3dl` file (red slowest, blue fastest; integer outputs). */
export function parse3dl({ text }: { text: string }): LutTable {
	let grid: number[] | null = null;
	const rows: number[][] = [];
	for (const raw of text.split(/\r?\n/)) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const numbers = line.split(/\s+/).map(Number).filter((v) => Number.isFinite(v));
		if (numbers.length === 0) continue;
		if (grid === null && numbers.length > 3) {
			grid = numbers;
			continue;
		}
		if (numbers.length >= 3) rows.push(numbers.slice(0, 3));
	}
	const size = grid ? grid.length : Math.round(Math.cbrt(rows.length));
	if (size < 2 || rows.length !== size ** 3) {
		throw new Error(`expected ${size ** 3} rows for a ${size}-point 3dl, found ${rows.length}`);
	}
	const peak = Math.max(...rows.flat());
	const scale = peak > 4095.5 ? 65535 : peak > 1023.5 ? 4095 : peak > 255.5 ? 1023 : peak > 1 ? 255 : 1;
	const rgb = new Float32Array(size * size * size * 3);
	rows.forEach((row, index) => {
		const r = Math.floor(index / (size * size));
		const g = Math.floor(index / size) % size;
		const b = index % size;
		const atIndex = ((b * size + g) * size + r) * 3;
		rgb[atIndex] = row[0] / scale;
		rgb[atIndex + 1] = row[1] / scale;
		rgb[atIndex + 2] = row[2] / scale;
	});
	return { size, rgb };
}

/** A HALD CLUT image (a level-L square of L³ pixels a side) as a table of L² points a side. */
export function fromHald({ rgba, width, height }: { rgba: Uint8ClampedArray | Uint8Array; width: number; height: number }): LutTable {
	if (width !== height || rgba.length !== width * height * 4) throw new Error("a HALD CLUT image is square");
	const level = Math.round(Math.cbrt(width));
	if (level < 2 || level ** 3 !== width) throw new Error(`${width}×${height} is not a HALD CLUT size (a level cubed)`);
	const size = level * level;
	const rgb = new Float32Array(size * size * size * 3);
	for (let index = 0; index < size * size * size; index++) {
		rgb[index * 3] = rgba[index * 4] / 255;
		rgb[index * 3 + 1] = rgba[index * 4 + 1] / 255;
		rgb[index * 3 + 2] = rgba[index * 4 + 2] / 255;
	}
	return { size, rgb };
}

function bytesToBase64(bytes: Uint8Array): string {
	let binary = "";
	for (let offset = 0; offset < bytes.length; offset += 0x8000) {
		binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
	}
	return btoa(binary);
}

function base64ToBytes(text: string): Uint8Array {
	const binary = atob(text);
	const bytes = new Uint8Array(binary.length);
	for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
	return bytes;
}

/** The `lut` effect parameter: `name|size|base64(RGB8)`, at most 33 a side. */
export function encodeLutParam({ name, table }: { name: string; table: LutTable }): string {
	const kept = table.size > MAX_LUT_SIZE ? resampleLut({ table, size: MAX_LUT_SIZE }) : table;
	const bytes = new Uint8Array(kept.rgb.length);
	for (let index = 0; index < kept.rgb.length; index++) bytes[index] = Math.round(Math.min(1, Math.max(0, kept.rgb[index])) * 255);
	return `${name.replace(/\|/g, "/")}|${kept.size}|${bytesToBase64(bytes)}`;
}

/** A decoded `lut` parameter, or `null` when it is empty or malformed. */
export function decodeLutParam({ param }: { param: string }): { name: string; table: LutTable } | null {
	const first = param.indexOf("|");
	const second = param.indexOf("|", first + 1);
	if (first < 0 || second < 0) return null;
	const name = param.slice(0, first);
	const size = Number.parseInt(param.slice(first + 1, second), 10);
	if (!Number.isInteger(size) || size < 2) return null;
	let bytes: Uint8Array;
	try {
		bytes = base64ToBytes(param.slice(second + 1));
	} catch {
		return null;
	}
	if (bytes.length !== size * size * size * 3) return null;
	const rgb = new Float32Array(bytes.length);
	for (let index = 0; index < bytes.length; index++) rgb[index] = bytes[index] / 255;
	return { name, table: { size, rgb } };
}

/** A short stable key for a `lut` parameter (FNV-1a of its text), used to cache the uploaded texture. */
export function lutKey({ param }: { param: string }): string {
	let hash = 0xcbf29ce484222325n;
	for (let index = 0; index < param.length; index++) {
		hash ^= BigInt(param.charCodeAt(index) & 0xff);
		hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
	}
	return `lut-${hash.toString(16).padStart(16, "0")}-${param.length}`;
}

/** Parses a LUT file by its extension: `.cube`, `.3dl`, or a HALD image (png, jpg, webp). */
export async function parseLutFile({ file }: { file: File }): Promise<{ name: string; table: LutTable }> {
	const name = file.name.replace(/\.[a-z0-9]+$/i, "");
	const lower = file.name.toLowerCase();
	if (lower.endsWith(".cube")) return { name, table: parseCube({ text: await file.text() }) };
	if (lower.endsWith(".3dl")) return { name, table: parse3dl({ text: await file.text() }) };
	if (/\.(png|jpe?g|webp|bmp)$/.test(lower)) {
		const bitmap = await createImageBitmap(file);
		const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
		const context = canvas.getContext("2d");
		if (!context) throw new Error("No 2D context to read the image");
		context.drawImage(bitmap, 0, 0);
		const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
		bitmap.close();
		return { name, table: fromHald({ rgba: pixels, width: bitmap.width, height: bitmap.height }) };
	}
	throw new Error("A LUT is a .cube or .3dl file or a HALD CLUT image");
}

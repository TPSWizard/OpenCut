/**
 * The colour and stylise effects, each one shader pass. Keys, defaults and
 * ranges match `crates/editor/src/effect_catalogue.rs`, and the pass
 * numbers match `crates/render/src/effects.rs`, so a project renders the
 * same on the desktop. Pixel-sized settings are given for a 1080-high
 * frame and scale with the frame's height, as blur does.
 */

import type { ParamDefinition, ParamValues } from "@/params";
import type { EffectDefinition, EffectPass } from "@/effects/types";
import { decodeLutParam, lutKey, lutTextureRgba } from "@/luts/parse";

const REFERENCE_HEIGHT = 1080;

function readNumber({
	params,
	key,
	fallback,
}: {
	params: ParamValues;
	key: string;
	fallback: number;
}): number {
	const raw = params[key];
	if (typeof raw === "number" && Number.isFinite(raw)) return raw;
	const parsed = Number.parseFloat(String(raw));
	return Number.isFinite(parsed) ? parsed : fallback;
}

function number({
	key,
	label,
	default: defaultValue,
	min,
	max,
	step,
}: {
	key: string;
	label: string;
	default: number;
	min: number;
	max: number;
	step: number;
}): ParamDefinition {
	return { key, label, type: "number", default: defaultValue, min, max, step };
}

const degreesToRadians = (degrees: number) => (degrees * Math.PI) / 180;

/** One-pass effect whose uniforms are read from its params. */
function singlePass({
	type,
	name,
	keywords,
	shader,
	params,
	uniforms,
}: {
	type: string;
	name: string;
	keywords: string[];
	shader: string;
	params: ParamDefinition[];
	uniforms: (context: {
		read: (key: string) => number;
		params: ParamValues;
		width: number;
		height: number;
		timeSeconds: number;
		pixelScale: number;
	}) => Record<string, number>;
}): EffectDefinition {
	const defaults = new Map(
		params.map((param) => [param.key, typeof param.default === "number" ? param.default : 0]),
	);
	const build = ({
		effectParams,
		width,
		height,
		timeSeconds,
	}: {
		effectParams: ParamValues;
		width: number;
		height: number;
		timeSeconds: number;
	}): EffectPass[] => [
		{
			shader,
			uniforms: uniforms({
				read: (key) =>
					readNumber({ params: effectParams, key, fallback: defaults.get(key) ?? 0 }),
				params: effectParams,
				width,
				height,
				timeSeconds,
				pixelScale: height / REFERENCE_HEIGHT,
			}),
		},
	];
	return {
		type,
		name,
		keywords,
		params,
		renderer: {
			passes: [
				{
					shader,
					uniforms: (context) => build(context)[0].uniforms,
				},
			],
			buildPasses: build,
		},
	};
}

export const colorEffectDefinition = singlePass({
	type: "color",
	name: "Colour correction",
	keywords: ["color", "colour", "brightness", "contrast", "saturation", "hue", "exposure", "gamma", "warmth", "tint", "grade"],
	shader: "color-correct",
	params: [
		number({ key: "brightness", label: "Brightness", default: 0, min: -1, max: 1, step: 0.01 }),
		number({ key: "contrast", label: "Contrast", default: 0, min: -1, max: 1, step: 0.01 }),
		number({ key: "saturation", label: "Saturation", default: 0, min: -1, max: 1, step: 0.01 }),
		number({ key: "hue", label: "Hue (°)", default: 0, min: -180, max: 180, step: 1 }),
		number({ key: "exposure", label: "Exposure (stops)", default: 0, min: -3, max: 3, step: 0.05 }),
		number({ key: "gamma", label: "Gamma", default: 1, min: 0.2, max: 3, step: 0.01 }),
		number({ key: "temperature", label: "Warmth", default: 0, min: -1, max: 1, step: 0.01 }),
		number({ key: "tint", label: "Tint", default: 0, min: -1, max: 1, step: 0.01 }),
	],
	uniforms: ({ read }) => ({
		u_brightness: read("brightness"),
		u_contrast: read("contrast"),
		u_saturation: read("saturation"),
		u_hue: degreesToRadians(read("hue")),
		u_exposure: read("exposure"),
		u_gamma: read("gamma"),
		u_temperature: read("temperature"),
		u_tint: read("tint"),
	}),
});

export const vignetteEffectDefinition = singlePass({
	type: "vignette",
	name: "Vignette",
	keywords: ["vignette", "corners", "dark", "frame"],
	shader: "vignette",
	params: [
		number({ key: "amount", label: "Amount", default: 0.5, min: 0, max: 1, step: 0.01 }),
		number({ key: "radius", label: "Radius", default: 0.6, min: 0, max: 1.5, step: 0.01 }),
		number({ key: "softness", label: "Softness", default: 0.5, min: 0, max: 1, step: 0.01 }),
	],
	uniforms: ({ read }) => ({
		u_amount: read("amount"),
		u_radius: read("radius"),
		u_softness: read("softness"),
	}),
});

export const grainEffectDefinition = singlePass({
	type: "grain",
	name: "Film grain",
	keywords: ["grain", "film", "noise", "texture"],
	shader: "film-grain",
	params: [
		number({ key: "amount", label: "Amount", default: 0.3, min: 0, max: 1, step: 0.01 }),
		number({ key: "size", label: "Grain size", default: 2, min: 1, max: 8, step: 0.5 }),
		number({ key: "seed", label: "Seed", default: 0, min: 0, max: 100, step: 1 }),
	],
	uniforms: ({ read, pixelScale, timeSeconds }) => ({
		u_amount: read("amount"),
		u_size: Math.max(1, read("size") * pixelScale),
		u_time: timeSeconds,
		u_seed: read("seed"),
	}),
});

export const rgbSplitEffectDefinition = singlePass({
	type: "rgb-split",
	name: "RGB split",
	keywords: ["rgb", "split", "chromatic", "aberration", "fringe"],
	shader: "rgb-split",
	params: [
		number({ key: "amount", label: "Offset (px)", default: 6, min: 0, max: 50, step: 0.5 }),
		number({ key: "angle", label: "Angle (°)", default: 0, min: 0, max: 360, step: 1 }),
	],
	uniforms: ({ read, pixelScale }) => ({
		u_amount: read("amount") * pixelScale,
		u_angle: degreesToRadians(read("angle")),
	}),
});

export const pixelateEffectDefinition = singlePass({
	type: "pixelate",
	name: "Pixelate",
	keywords: ["pixelate", "mosaic", "blocks", "censor"],
	shader: "pixelate",
	params: [
		number({ key: "size", label: "Cell size (px)", default: 16, min: 2, max: 200, step: 1 }),
	],
	uniforms: ({ read, pixelScale }) => ({
		u_size: Math.max(1, read("size") * pixelScale),
	}),
});

export const sharpenEffectDefinition = singlePass({
	type: "sharpen",
	name: "Sharpen",
	keywords: ["sharpen", "detail", "crisp", "edges"],
	shader: "sharpen",
	params: [number({ key: "amount", label: "Amount", default: 0.5, min: 0, max: 1, step: 0.01 })],
	uniforms: ({ read }) => ({ u_amount: read("amount") }),
});

export const MIRROR_MODES = ["left", "right", "top", "bottom"] as const;

export const mirrorEffectDefinition = singlePass({
	type: "mirror",
	name: "Mirror",
	keywords: ["mirror", "reflect", "flip", "symmetry"],
	shader: "mirror",
	params: [
		{
			key: "mode",
			label: "Side",
			type: "select",
			default: "left",
			options: [
				{ value: "left", label: "Left onto right" },
				{ value: "right", label: "Right onto left" },
				{ value: "top", label: "Top onto bottom" },
				{ value: "bottom", label: "Bottom onto top" },
			],
		},
	],
	uniforms: ({ params }) => {
		const index = MIRROR_MODES.indexOf(String(params.mode) as (typeof MIRROR_MODES)[number]);
		return { u_mode: index < 0 ? 0 : index };
	},
});

export const glitchEffectDefinition = singlePass({
	type: "glitch",
	name: "Glitch",
	keywords: ["glitch", "tear", "digital", "broken", "vhs"],
	shader: "glitch",
	params: [
		number({ key: "amount", label: "Amount", default: 0.5, min: 0, max: 1, step: 0.01 }),
		number({ key: "bands", label: "Bands", default: 16, min: 4, max: 64, step: 1 }),
		number({ key: "seed", label: "Seed", default: 0, min: 0, max: 100, step: 1 }),
	],
	uniforms: ({ read, timeSeconds }) => ({
		u_amount: read("amount"),
		u_bands: read("bands"),
		u_time: timeSeconds,
		u_seed: read("seed"),
	}),
});

export const kaleidoscopeEffectDefinition = singlePass({
	type: "kaleidoscope",
	name: "Kaleidoscope",
	keywords: ["kaleidoscope", "wedges", "symmetry", "pattern"],
	shader: "kaleidoscope",
	params: [
		number({ key: "segments", label: "Segments", default: 6, min: 2, max: 16, step: 1 }),
		number({ key: "angle", label: "Rotation (°)", default: 0, min: 0, max: 360, step: 1 }),
	],
	uniforms: ({ read }) => ({
		u_segments: read("segments"),
		u_angle: degreesToRadians(read("angle")),
	}),
});

const unit = ({ key, label, default: defaultValue = 0 }: { key: string; label: string; default?: number }) =>
	number({ key, label, default: defaultValue, min: -1, max: 1, step: 0.01 });

const level = ({ key, label, default: defaultValue }: { key: string; label: string; default: number }) =>
	number({ key, label, default: defaultValue, min: 0, max: 1, step: 0.01 });

export const liftGammaGainEffectDefinition = singlePass({
	type: "lift-gamma-gain",
	name: "Lift, gamma, gain",
	keywords: ["lift", "gamma", "gain", "three-way", "color", "colour", "grade", "shadows", "midtones", "highlights"],
	shader: "lift-gamma-gain",
	params: [
		unit({ key: "lift", label: "Lift" }),
		unit({ key: "liftRed", label: "Lift red" }),
		unit({ key: "liftGreen", label: "Lift green" }),
		unit({ key: "liftBlue", label: "Lift blue" }),
		number({ key: "gamma", label: "Gamma", default: 1, min: 0.2, max: 3, step: 0.01 }),
		unit({ key: "gammaRed", label: "Gamma red" }),
		unit({ key: "gammaGreen", label: "Gamma green" }),
		unit({ key: "gammaBlue", label: "Gamma blue" }),
		number({ key: "gain", label: "Gain", default: 1, min: 0, max: 3, step: 0.01 }),
		unit({ key: "gainRed", label: "Gain red" }),
		unit({ key: "gainGreen", label: "Gain green" }),
		unit({ key: "gainBlue", label: "Gain blue" }),
	],
	uniforms: ({ read }) => ({
		u_lift: read("lift"),
		u_lift_r: read("liftRed"),
		u_lift_g: read("liftGreen"),
		u_lift_b: read("liftBlue"),
		u_gamma: read("gamma"),
		u_gamma_r: read("gammaRed"),
		u_gamma_g: read("gammaGreen"),
		u_gamma_b: read("gammaBlue"),
		u_gain: read("gain"),
		u_gain_r: read("gainRed"),
		u_gain_g: read("gainGreen"),
		u_gain_b: read("gainBlue"),
	}),
});

export const curvesEffectDefinition = singlePass({
	type: "curves",
	name: "Curves",
	keywords: ["curves", "tone", "rgb", "color", "colour", "grade", "contrast"],
	shader: "curves",
	params: [
		level({ key: "master0", label: "Master at 0 %", default: 0 }),
		level({ key: "master25", label: "Master at 25 %", default: 0.25 }),
		level({ key: "master50", label: "Master at 50 %", default: 0.5 }),
		level({ key: "master75", label: "Master at 75 %", default: 0.75 }),
		level({ key: "master100", label: "Master at 100 %", default: 1 }),
		level({ key: "red25", label: "Red at 25 %", default: 0.25 }),
		level({ key: "red50", label: "Red at 50 %", default: 0.5 }),
		level({ key: "red75", label: "Red at 75 %", default: 0.75 }),
		level({ key: "green25", label: "Green at 25 %", default: 0.25 }),
		level({ key: "green50", label: "Green at 50 %", default: 0.5 }),
		level({ key: "green75", label: "Green at 75 %", default: 0.75 }),
		level({ key: "blue25", label: "Blue at 25 %", default: 0.25 }),
		level({ key: "blue50", label: "Blue at 50 %", default: 0.5 }),
		level({ key: "blue75", label: "Blue at 75 %", default: 0.75 }),
	],
	uniforms: ({ read }) => ({
		u_master_0: read("master0"),
		u_master_25: read("master25"),
		u_master_50: read("master50"),
		u_master_75: read("master75"),
		u_master_100: read("master100"),
		u_red_25: read("red25"),
		u_red_50: read("red50"),
		u_red_75: read("red75"),
		u_green_25: read("green25"),
		u_green_50: read("green50"),
		u_green_75: read("green75"),
		u_blue_25: read("blue25"),
		u_blue_50: read("blue50"),
		u_blue_75: read("blue75"),
	}),
});

export const levelsEffectDefinition = singlePass({
	type: "levels",
	name: "Levels",
	keywords: ["levels", "black point", "white point", "gamma", "color", "colour", "contrast"],
	shader: "levels",
	params: [
		level({ key: "inputBlack", label: "Input black", default: 0 }),
		level({ key: "inputWhite", label: "Input white", default: 1 }),
		number({ key: "gamma", label: "Gamma", default: 1, min: 0.1, max: 10, step: 0.01 }),
		level({ key: "outputBlack", label: "Output black", default: 0 }),
		level({ key: "outputWhite", label: "Output white", default: 1 }),
	],
	uniforms: ({ read }) => ({
		u_in_black: read("inputBlack"),
		u_in_white: read("inputWhite"),
		u_gamma: read("gamma"),
		u_out_black: read("outputBlack"),
		u_out_white: read("outputWhite"),
	}),
});

export const shadowsHighlightsEffectDefinition = singlePass({
	type: "shadows-highlights",
	name: "Shadows and highlights",
	keywords: ["shadows", "highlights", "recover", "lift", "color", "colour", "tone"],
	shader: "shadows-highlights",
	params: [
		unit({ key: "shadows", label: "Shadows" }),
		unit({ key: "highlights", label: "Highlights" }),
		level({ key: "range", label: "Range", default: 0.5 }),
	],
	uniforms: ({ read }) => ({
		u_shadows: read("shadows"),
		u_highlights: read("highlights"),
		u_range: read("range"),
	}),
});

/** Decoded tables by parameter key; a handful stay cached across frames. */
const lutCache = new Map<string, { size: number; data: Uint8Array }>();

function lutPayload({ param }: { param: string }): { key: string; size: number; data: Uint8Array } | null {
	if (!param) return null;
	const key = lutKey({ param });
	const cached = lutCache.get(key);
	if (cached) return { key, ...cached };
	const decoded = decodeLutParam({ param });
	if (!decoded) return null;
	const entry = { size: decoded.table.size, data: lutTextureRgba({ table: decoded.table }) };
	if (lutCache.size >= 8) {
		const oldest = lutCache.keys().next().value;
		if (oldest !== undefined) lutCache.delete(oldest);
	}
	lutCache.set(key, entry);
	return { key, ...entry };
}

const LUT_INTERPOLATIONS = ["trilinear", "tetrahedral"] as const;

function buildLutPasses({ effectParams }: { effectParams: ParamValues }): EffectPass[] {
	const payload = lutPayload({ param: typeof effectParams.lut === "string" ? effectParams.lut : "" });
	if (!payload) return [];
	const intensity = readNumber({ params: effectParams, key: "intensity", fallback: 1 });
	return [
		{
			shader: "lut",
			uniforms: {
				u_size: payload.size,
				u_intensity: Math.min(1, Math.max(0, intensity)),
				u_tetrahedral: effectParams.interpolation === "tetrahedral" ? 1 : 0,
			},
			lut: payload,
		},
	];
}

export const lutEffectDefinition: EffectDefinition = {
	type: "lut",
	name: "LUT",
	keywords: ["lut", "cube", "3dl", "hald", "look", "grade", "color", "colour", "film"],
	params: [
		number({ key: "intensity", label: "Intensity", default: 1, min: 0, max: 1, step: 0.01 }),
		{
			key: "interpolation",
			label: "Interpolation",
			type: "select",
			default: "trilinear",
			options: LUT_INTERPOLATIONS.map((value) => ({ value, label: value === "trilinear" ? "Trilinear" : "Tetrahedral" })),
		},
		{ key: "lut", label: "Table", type: "text", default: "" },
	],
	renderer: {
		passes: [{ shader: "lut", uniforms: (context) => buildLutPasses(context)[0]?.uniforms ?? { u_size: 2, u_intensity: 0, u_tetrahedral: 0 } }],
		buildPasses: buildLutPasses,
	},
};

/** "#rrggbb" (or "#rgb") to 0..1 channels; the default green when unreadable. */
export function keyColorChannels(value: unknown): [number, number, number] {
	const hex = String(value ?? "").trim().replace(/^#/, "");
	const full = hex.length === 3 || hex.length === 4 ? [...hex.slice(0, 3)].map((digit) => digit + digit).join("") : hex.slice(0, 6);
	if (!/^[0-9a-fA-F]{6}$/.test(full)) return [0, 0xb1 / 255, 0x40 / 255];
	return [0, 2, 4].map((start) => Number.parseInt(full.slice(start, start + 2), 16) / 255) as [number, number, number];
}

/**
 * Retouch: smooths, brightens and evens skin, found by its colour, with an
 * edge-preserving blur; what is not skin is left as it is. The shared
 * `retouch` shader (`crates/effects`), as on the desktop.
 */
export const RETOUCH_FACE_UNIFORMS = [
	"u_face0_x", "u_face0_y", "u_face0_rx", "u_face0_ry",
	"u_face1_x", "u_face1_y", "u_face1_rx", "u_face1_ry",
	"u_face2_x", "u_face2_y", "u_face2_rx", "u_face2_ry",
] as const;

const retouchPass = singlePass({
	type: "retouch",
	name: "Retouch",
	keywords: ["retouch", "beauty", "skin", "smooth", "face", "soften", "complexion"],
	shader: "retouch",
	params: [
		number({ key: "smooth", label: "Smooth", default: 0.6, min: 0, max: 1, step: 0.01 }),
		number({ key: "brighten", label: "Brighten", default: 0.3, min: 0, max: 1, step: 0.01 }),
		number({ key: "even", label: "Even tone", default: 0.4, min: 0, max: 1, step: 0.01 }),
		number({ key: "radius", label: "Radius (px)", default: 12, min: 2, max: 40, step: 1 }),
		{
			key: "area",
			label: "Area",
			type: "select",
			default: "skin",
			options: [
				{ value: "skin", label: "Skin everywhere" },
				{ value: "faces", label: "Faces only" },
			],
		},
	],
	uniforms: ({ read, pixelScale }) => ({
		u_smooth: Math.min(1, Math.max(0, read("smooth"))),
		u_brighten: Math.min(1, Math.max(0, read("brighten"))),
		u_even: Math.min(1, Math.max(0, read("even"))),
		u_step: Math.max(1, Math.round((read("radius") * pixelScale) / 2)),
		...Object.fromEntries(RETOUCH_FACE_UNIFORMS.map((name) => [name, 0])),
	}),
});

/** Area "faces": the pass is marked so the frame fills in the layer's face ovals. */
export const retouchEffectDefinition: EffectDefinition = {
	...retouchPass,
	renderer: {
		...retouchPass.renderer,
		buildPasses: (context) =>
			(retouchPass.renderer.buildPasses?.(context) ?? []).map((pass) =>
				context.effectParams.area === "faces" ? { ...pass, faces: true } : pass,
			),
	},
};

/** Face blur's styles, in the shader's order (`u_style`). */
const FACE_BLUR_STYLES = ["blur", "pixelate", "black"] as const;

const faceBlurPass = singlePass({
	type: "face-blur",
	name: "Face blur",
	keywords: ["face blur", "blur faces", "privacy", "anonymise", "anonymize", "mosaic", "pixelate face", "censor", "hide face"],
	shader: "face-blur",
	params: [
		{
			key: "style",
			label: "Style",
			type: "select",
			default: "blur",
			options: [
				{ value: "blur", label: "Blur" },
				{ value: "pixelate", label: "Pixelate" },
				{ value: "black", label: "Black" },
			],
		},
		number({ key: "strength", label: "Strength", default: 1, min: 0.1, max: 2, step: 0.05 }),
		number({ key: "size", label: "Size", default: 1.2, min: 0.8, max: 2.5, step: 0.05 }),
		number({ key: "feather", label: "Soft edge", default: 0.2, min: 0, max: 0.6, step: 0.01 }),
	],
	uniforms: ({ read, params }) => ({
		u_style: Math.max(0, FACE_BLUR_STYLES.indexOf(params.style as (typeof FACE_BLUR_STYLES)[number])),
		u_strength: Math.min(2, Math.max(0, read("strength"))),
		u_size: Math.min(4, Math.max(0.1, read("size"))),
		u_feather: Math.min(1, Math.max(0, read("feather"))),
		...Object.fromEntries(RETOUCH_FACE_UNIFORMS.map((name) => [name, 0])),
	}),
});

/**
 * Face blur (`face-blur` shader, `crates/effects`): hides the faces of the
 * clip's face track with a blur (two passes, across then down), pixels or
 * black, as on the desktop. Its passes always work on faces only, so the
 * frame fills in the layer's face ovals and leaves them out with none.
 */
export const faceBlurEffectDefinition: EffectDefinition = {
	...faceBlurPass,
	renderer: {
		...faceBlurPass.renderer,
		buildPasses: (context) =>
			(faceBlurPass.renderer.buildPasses?.(context) ?? []).flatMap((pass) => {
				const marked = { ...pass, faces: true };
				if (pass.uniforms.u_style !== 0) return [marked];
				return [
					{ ...marked, uniforms: { ...marked.uniforms, u_direction: [1, 0] } },
					{ ...marked, uniforms: { ...marked.uniforms, u_direction: [0, 1] } },
				];
			}),
	},
};

export const chromaKeyEffectDefinition = singlePass({
	type: "chroma-key",
	name: "Chroma key",
	keywords: ["chroma", "key", "green screen", "blue screen", "keying", "transparent", "remove colour", "remove color"],
	shader: "chroma-key",
	params: [
		{ key: "keyColor", label: "Key colour", type: "color", default: "#00b140" },
		number({ key: "strength", label: "Strength", default: 0.4, min: 0, max: 1.5, step: 0.01 }),
		number({ key: "softness", label: "Edge softness", default: 0.15, min: 0, max: 1, step: 0.01 }),
		number({ key: "spill", label: "Spill removal", default: 0.5, min: 0, max: 1, step: 0.01 }),
	],
	uniforms: ({ read, params }) => {
		const [red, green, blue] = keyColorChannels(params.keyColor);
		return {
			u_key_r: red,
			u_key_g: green,
			u_key_b: blue,
			u_strength: read("strength"),
			u_softness: read("softness"),
			u_spill: read("spill"),
		};
	},
});

export const libraryEffectDefinitions: EffectDefinition[] = [
	colorEffectDefinition,
	vignetteEffectDefinition,
	grainEffectDefinition,
	rgbSplitEffectDefinition,
	pixelateEffectDefinition,
	sharpenEffectDefinition,
	mirrorEffectDefinition,
	glitchEffectDefinition,
	kaleidoscopeEffectDefinition,
	liftGammaGainEffectDefinition,
	curvesEffectDefinition,
	levelsEffectDefinition,
	shadowsHighlightsEffectDefinition,
	chromaKeyEffectDefinition,
	retouchEffectDefinition,
	faceBlurEffectDefinition,
	lutEffectDefinition,
];

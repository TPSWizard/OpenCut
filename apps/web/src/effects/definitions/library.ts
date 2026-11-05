/**
 * The colour and stylise effects, each one shader pass. Keys, defaults and
 * ranges match `crates/editor/src/effect_catalogue.rs`, and the pass
 * numbers match `crates/render/src/effects.rs`, so a project renders the
 * same on the desktop. Pixel-sized settings are given for a 1080-high
 * frame and scale with the frame's height, as blur does.
 */

import type { ParamDefinition, ParamValues } from "@/params";
import type { EffectDefinition, EffectPass } from "@/effects/types";

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
];

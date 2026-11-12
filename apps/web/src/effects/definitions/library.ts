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
];

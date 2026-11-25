import { ZOOM_FOCUS_Y_DEFAULT, ZOOM_RAMP_DEFAULT, ZOOM_RAMP_MAX } from "@/auto-zoom/zoom";
import { CORNER_PIN_ENABLED, CORNER_PIN_MAX, CORNER_PIN_MIN, PIN_CORNERS, cornerPinKey } from "@/corner-pin/pin";
import type {
	ParamDefinition,
	ParamValue,
	ParamValues,
} from "@/params";
import { MIN_TRANSFORM_SCALE } from "@/animation/transform";
import type { BlendMode } from "@/rendering";
import type {
	ElementType,
	TimelineElement,
} from "@/timeline";
import { DEFAULTS } from "@/timeline/defaults";
import { VOLUME_DB_MAX, VOLUME_DB_MIN } from "@/timeline/audio-constants";
import {
	CORNER_RADIUS_MAX,
	CORNER_RADIUS_MIN,
} from "@/text/background";
import { DEFAULT_CAPTION_STYLE } from "@/captions/params";
import {
	ANIMATION_DURATION_DEFAULT,
	ANIMATION_DURATION_MAX,
	ANIMATION_DURATION_MIN,
	ANIMATION_IN_KINDS,
	ANIMATION_LABELS,
	ANIMATION_LOOP_KINDS,
	ANIMATION_LOOP_PERIOD_DEFAULT,
	ANIMATION_LOOP_PERIOD_MAX,
	ANIMATION_LOOP_PERIOD_MIN,
	ANIMATION_OUT_KINDS,
	TEXT_ONLY_ANIMATIONS,
} from "@/clip-animation/presets";
import {
	CORNER_RADIUS_MAX as LAYER_CORNER_RADIUS_MAX,
	OUTLINE_WIDTH_MAX,
	SHADOW_BLUR_DEFAULT,
	SHADOW_BLUR_MAX,
	SHADOW_OFFSET_DEFAULT,
	SHADOW_OFFSET_MAX,
	SHADOW_OPACITY_DEFAULT,
} from "@/layer-style/style";

export type ElementParamDefinition<TKey extends string = string> =
	ParamDefinition<TKey> & {
		read?: ({ element }: { element: TimelineElement }) => ParamValue | null;
		write?: ({
			element,
			value,
		}: {
			element: TimelineElement;
			value: ParamValue;
		}) => TimelineElement;
	};

export function buildDefaultParamValues(
	params: readonly ParamDefinition[],
): ParamValues {
	const values: ParamValues = {};
	for (const param of params) {
		values[param.key] = param.default;
	}
	return values;
}

export class DefinitionRegistry<TKey extends string, TDefinition> {
	private definitions = new Map<TKey, TDefinition>();
	private entityName: string;

	constructor(entityName: string) {
		this.entityName = entityName;
	}

	register({
		key,
		definition,
	}: {
		key: TKey;
		definition: TDefinition;
	}): void {
		this.definitions.set(key, definition);
	}

	has(key: TKey): boolean {
		return this.definitions.has(key);
	}

	get(key: TKey): TDefinition {
		const def = this.definitions.get(key);
		if (!def) {
			throw new Error(`Unknown ${this.entityName}: ${key}`);
		}
		return def;
	}

	getAll(): TDefinition[] {
		return Array.from(this.definitions.values());
	}
}

const BLEND_MODE_OPTIONS: Array<{ value: BlendMode; label: string }> = [
	{ value: "normal", label: "Normal" },
	{ value: "darken", label: "Darken" },
	{ value: "multiply", label: "Multiply" },
	{ value: "color-burn", label: "Color Burn" },
	{ value: "lighten", label: "Lighten" },
	{ value: "screen", label: "Screen" },
	{ value: "plus-lighter", label: "Plus Lighter" },
	{ value: "color-dodge", label: "Color Dodge" },
	{ value: "overlay", label: "Overlay" },
	{ value: "soft-light", label: "Soft Light" },
	{ value: "hard-light", label: "Hard Light" },
	{ value: "difference", label: "Difference" },
	{ value: "exclusion", label: "Exclusion" },
	{ value: "hue", label: "Hue" },
	{ value: "saturation", label: "Saturation" },
	{ value: "color", label: "Color" },
	{ value: "luminosity", label: "Luminosity" },
];

const visualElementParams: ElementParamDefinition[] = [
	{
		key: "transform.positionX",
		label: "Position X",
		type: "number",
		default: DEFAULTS.element.transform.position.x,
		min: -100_000,
		step: 1,
	},
	{
		key: "transform.positionY",
		label: "Position Y",
		type: "number",
		default: DEFAULTS.element.transform.position.y,
		min: -100_000,
		step: 1,
	},
	{
		key: "transform.scaleX",
		label: "Scale X",
		type: "number",
		default: DEFAULTS.element.transform.scaleX,
		min: MIN_TRANSFORM_SCALE,
		step: 0.01,
	},
	{
		key: "transform.scaleY",
		label: "Scale Y",
		type: "number",
		default: DEFAULTS.element.transform.scaleY,
		min: MIN_TRANSFORM_SCALE,
		step: 0.01,
	},
	{
		key: "transform.rotate",
		label: "Rotate",
		type: "number",
		default: DEFAULTS.element.transform.rotate,
		min: -360,
		max: 360,
		step: 1,
	},
	{
		// Motion blur (`@/motion-blur/blur`): how much of a frame the
		// shutter stays open, 0 (off) to 100.
		key: "motionBlur",
		label: "Motion blur",
		type: "number",
		default: 0,
		min: 0,
		max: 100,
		step: 1,
		keyframable: false,
	},
	{
		key: "opacity",
		label: "Opacity",
		type: "number",
		default: DEFAULTS.element.opacity,
		min: 0,
		max: 1,
		step: 0.01,
	},
	{
		key: "blendMode",
		label: "Blend Mode",
		type: "select",
		default: DEFAULTS.element.blendMode,
		keyframable: false,
		options: BLEND_MODE_OPTIONS,
	},
];

function animationOptions(kinds: readonly string[], isText: boolean): Array<{ value: string; label: string }> {
	return [
		{ value: "none", label: "None" },
		...kinds
			.filter((kind) => isText || !TEXT_ONLY_ANIMATIONS.has(kind))
			.map((kind) => ({ value: kind, label: ANIMATION_LABELS[kind] ?? kind })),
	];
}

/** One-click animations (see `@/clip-animation/presets`); typewriter and word by word only on text. */
function animationParams({ isText }: { isText: boolean }): ElementParamDefinition[] {
	return [
		{
			key: "animIn",
			label: "Animation in",
			type: "select",
			default: "none",
			keyframable: false,
			options: animationOptions(ANIMATION_IN_KINDS, isText),
		},
		{
			key: "animInDuration",
			label: "In length (s)",
			type: "number",
			default: ANIMATION_DURATION_DEFAULT,
			min: ANIMATION_DURATION_MIN,
			max: ANIMATION_DURATION_MAX,
			step: 0.1,
			keyframable: false,
		},
		{
			key: "animOut",
			label: "Animation out",
			type: "select",
			default: "none",
			keyframable: false,
			options: animationOptions(ANIMATION_OUT_KINDS, isText),
		},
		{
			key: "animOutDuration",
			label: "Out length (s)",
			type: "number",
			default: ANIMATION_DURATION_DEFAULT,
			min: ANIMATION_DURATION_MIN,
			max: ANIMATION_DURATION_MAX,
			step: 0.1,
			keyframable: false,
		},
		{
			key: "animLoop",
			label: "Loop animation",
			type: "select",
			default: "none",
			keyframable: false,
			options: animationOptions(ANIMATION_LOOP_KINDS, isText),
		},
		{
			key: "animLoopPeriod",
			label: "Loop speed (s per cycle)",
			type: "number",
			default: ANIMATION_LOOP_PERIOD_DEFAULT,
			min: ANIMATION_LOOP_PERIOD_MIN,
			max: ANIMATION_LOOP_PERIOD_MAX,
			step: 0.1,
			keyframable: false,
		},
	];
}

/** Rounded corners (not on text), outline and drop shadow; see `@/layer-style/style`. */
/** Auto zoom punch-ins (video and picture clips); see `@/auto-zoom/zoom`. */
const zoomParams: ElementParamDefinition[] = [
	{ key: "zoom.punches", label: "Punch-ins (start-end@zoom)", type: "text", default: "", keyframable: false },
	{ key: "zoom.focusX", label: "Zoom focus X", type: "number", default: 0, min: -0.5, max: 0.5, step: 0.01, keyframable: false },
	{
		key: "zoom.focusY",
		label: "Zoom focus Y",
		type: "number",
		default: ZOOM_FOCUS_Y_DEFAULT,
		min: -0.5,
		max: 0.5,
		step: 0.01,
		keyframable: false,
	},
	{ key: "zoom.ramp", label: "Zoom ease (s)", type: "number", default: ZOOM_RAMP_DEFAULT, min: 0, max: ZOOM_RAMP_MAX, step: 0.05, keyframable: false },
];

/**
 * Corner pin (video, picture and sticker clips): each corner's place in
 * percent of the clip's own box, keyframable; see `@/corner-pin/pin`.
 */
const cornerPinParamDefinitions: ElementParamDefinition[] = [
	{ key: CORNER_PIN_ENABLED, label: "Corner pin", type: "boolean", default: false, keyframable: false },
	...PIN_CORNERS.flatMap(({ corner, label, default: place }) =>
		(["X", "Y"] as const).map(
			(axis, i): ElementParamDefinition => ({
				key: cornerPinKey(corner, axis),
				label: `${label} ${axis} (%)`,
				type: "number",
				default: place[i],
				min: CORNER_PIN_MIN,
				max: CORNER_PIN_MAX,
				step: 0.1,
				dependencies: [{ param: CORNER_PIN_ENABLED, equals: true }],
			}),
		),
	),
];

function styleParams({ isText }: { isText: boolean }): ElementParamDefinition[] {
	const shadowOn = [{ param: "shadow.enabled", equals: true }];
	return [
		...(isText
			? []
			: [
					{
						key: "cornerRadius",
						label: "Rounded corners (%)",
						type: "number",
						default: 0,
						min: 0,
						max: LAYER_CORNER_RADIUS_MAX,
						step: 1,
						keyframable: false,
					} satisfies ElementParamDefinition,
				]),
		{
			key: "outline.width",
			label: "Outline width",
			type: "number",
			default: 0,
			min: 0,
			max: OUTLINE_WIDTH_MAX,
			step: 0.5,
			keyframable: false,
		},
		{ key: "outline.color", label: "Outline colour", type: "color", default: "#ffffff", keyframable: false },
		{ key: "shadow.enabled", label: "Shadow", type: "boolean", default: false, keyframable: false },
		{
			key: "shadow.color",
			label: "Shadow colour",
			type: "color",
			default: "#000000",
			keyframable: false,
			dependencies: shadowOn,
		},
		{
			key: "shadow.opacity",
			label: "Shadow opacity",
			type: "number",
			default: SHADOW_OPACITY_DEFAULT,
			min: 0,
			max: 1,
			step: 0.01,
			keyframable: false,
			dependencies: shadowOn,
		},
		{
			key: "shadow.blur",
			label: "Shadow blur",
			type: "number",
			default: SHADOW_BLUR_DEFAULT,
			min: 0,
			max: SHADOW_BLUR_MAX,
			step: 1,
			keyframable: false,
			dependencies: shadowOn,
		},
		{
			key: "shadow.offsetX",
			label: "Shadow X",
			type: "number",
			default: SHADOW_OFFSET_DEFAULT,
			min: -SHADOW_OFFSET_MAX,
			max: SHADOW_OFFSET_MAX,
			step: 1,
			keyframable: false,
			dependencies: shadowOn,
		},
		{
			key: "shadow.offsetY",
			label: "Shadow Y",
			type: "number",
			default: SHADOW_OFFSET_DEFAULT,
			min: -SHADOW_OFFSET_MAX,
			max: SHADOW_OFFSET_MAX,
			step: 1,
			keyframable: false,
			dependencies: shadowOn,
		},
	];
}

const audioElementParams: ElementParamDefinition[] = [
	{
		key: "volume",
		label: "Volume",
		type: "number",
		default: DEFAULTS.element.volume,
		min: VOLUME_DB_MIN,
		max: VOLUME_DB_MAX,
		step: 0.01,
	},
	{
		key: "muted",
		label: "Muted",
		type: "boolean",
		default: false,
		keyframable: false,
	},
	{
		key: "fadeIn",
		label: "Fade in (s)",
		type: "number",
		default: 0,
		min: 0,
		max: 10,
		step: 0.1,
		keyframable: false,
	},
	{
		key: "fadeOut",
		label: "Fade out (s)",
		type: "number",
		default: 0,
		min: 0,
		max: 10,
		step: 0.1,
		keyframable: false,
	},
	{
		key: "pan",
		label: "Pan (left to right)",
		type: "number",
		default: 0,
		min: -1,
		max: 1,
		step: 0.01,
		keyframable: false,
	},
	// Audio effects (`@/audio-fx`): equaliser, compressor, limiter.
	{ key: "audio.eq.low", label: "Bass (dB)", type: "number", default: 0, min: -12, max: 12, step: 0.5, keyframable: false },
	{ key: "audio.eq.mid", label: "Mids (dB)", type: "number", default: 0, min: -12, max: 12, step: 0.5, keyframable: false },
	{ key: "audio.eq.high", label: "Treble (dB)", type: "number", default: 0, min: -12, max: 12, step: 0.5, keyframable: false },
	{ key: "audio.comp.enabled", label: "Compressor", type: "boolean", default: false, keyframable: false },
	{
		key: "audio.comp.threshold",
		label: "Compressor threshold (dB)",
		type: "number",
		default: -24,
		min: -60,
		max: 0,
		step: 1,
		keyframable: false,
		dependencies: [{ param: "audio.comp.enabled", equals: true }],
	},
	{
		key: "audio.comp.ratio",
		label: "Compressor ratio",
		type: "number",
		default: 4,
		min: 1,
		max: 20,
		step: 0.5,
		keyframable: false,
		dependencies: [{ param: "audio.comp.enabled", equals: true }],
	},
	{ key: "audio.limit.enabled", label: "Limiter", type: "boolean", default: false, keyframable: false },
	{
		key: "audio.limit.ceiling",
		label: "Limiter ceiling (dB)",
		type: "number",
		default: -1,
		min: -12,
		max: 0,
		step: 0.5,
		keyframable: false,
		dependencies: [{ param: "audio.limit.enabled", equals: true }],
	},
];

/** How a caption's spoken word is shown; see `@/captions`. */
const captionParams: ElementParamDefinition[] = [
	{
		key: "caption.highlight",
		label: "Highlight",
		type: "select",
		default: DEFAULT_CAPTION_STYLE.highlight,
		keyframable: false,
		options: [
			{ value: "none", label: "None" },
			{ value: "word", label: "Active word" },
			{ value: "box", label: "Active word in a box" },
			{ value: "fill", label: "Spoken so far (karaoke)" },
		],
	},
	{
		key: "caption.activeColor",
		label: "Active Color",
		type: "color",
		default: DEFAULT_CAPTION_STYLE.activeColor,
		keyframable: false,
	},
	{
		key: "caption.boxColor",
		label: "Box Color",
		type: "color",
		default: DEFAULT_CAPTION_STYLE.boxColor,
		keyframable: false,
	},
	{
		key: "caption.keywordColor",
		label: "Keyword Color",
		type: "color",
		default: DEFAULT_CAPTION_STYLE.keywordColor,
		keyframable: false,
	},
	{
		key: "caption.keywordScale",
		label: "Keyword Size",
		type: "number",
		default: DEFAULT_CAPTION_STYLE.keywordScale,
		min: 1,
		max: 2,
		step: 0.05,
		keyframable: false,
	},
];

const textElementParams: ElementParamDefinition[] = [
	{
		key: "content",
		label: "Content",
		type: "text",
		default: "Default text",
		keyframable: false,
	},
	{
		key: "fontFamily",
		label: "Font Family",
		type: "font",
		default: "Arial",
		keyframable: false,
	},
	{
		key: "fontSize",
		label: "Font Size",
		type: "number",
		default: 15,
		min: 1,
		step: 1,
	},
	{
		key: "color",
		label: "Color",
		type: "color",
		default: "#ffffff",
	},
	{
		key: "textAlign",
		label: "Text Align",
		type: "select",
		default: "center",
		keyframable: false,
		options: [
			{ value: "left", label: "Left" },
			{ value: "center", label: "Center" },
			{ value: "right", label: "Right" },
		],
	},
	{
		key: "fontWeight",
		label: "Font Weight",
		type: "select",
		default: "normal",
		keyframable: false,
		options: [
			{ value: "normal", label: "Normal" },
			{ value: "bold", label: "Bold" },
		],
	},
	{
		key: "fontStyle",
		label: "Font Style",
		type: "select",
		default: "normal",
		keyframable: false,
		options: [
			{ value: "normal", label: "Normal" },
			{ value: "italic", label: "Italic" },
		],
	},
	{
		key: "textDecoration",
		label: "Text Decoration",
		type: "select",
		default: "none",
		keyframable: false,
		options: [
			{ value: "none", label: "None" },
			{ value: "underline", label: "Underline" },
			{ value: "line-through", label: "Line Through" },
		],
	},
	{
		key: "letterSpacing",
		label: "Letter Spacing",
		type: "number",
		default: DEFAULTS.text.letterSpacing,
		min: -100,
		step: 0.1,
	},
	{
		key: "lineHeight",
		label: "Line Height",
		type: "number",
		default: DEFAULTS.text.lineHeight,
		min: 0.1,
		step: 0.1,
	},
	{
		// Curved text (`@/text/curve`): an arch above 0, a smile below.
		key: "curve",
		label: "Curve",
		type: "number",
		default: 0,
		min: -100,
		max: 100,
		step: 1,
	},
	{
		// Text on a path (`@/text/curve`, `textwarp::warp_path`): a shape the
		// text runs along, in place of the curve. `textPathPoints` holds a
		// custom path's control points (set through the API).
		key: "textPath",
		label: "Path",
		type: "select",
		default: "none",
		keyframable: false,
		options: [
			{ value: "none", label: "None" },
			{ value: "wave", label: "Wave" },
			{ value: "rise", label: "Rise" },
			{ value: "swoosh", label: "Swoosh" },
			{ value: "circle", label: "Circle" },
			{ value: "custom", label: "Custom" },
		],
	},
	{
		key: "textPathAmount",
		label: "Path amount",
		type: "number",
		default: 50,
		min: 0,
		max: 100,
		step: 1,
		keyframable: false,
	},
	{
		key: "background.enabled",
		label: "Background Enabled",
		type: "boolean",
		default: DEFAULTS.text.background.enabled,
		keyframable: false,
	},
	{
		key: "background.color",
		label: "Background Color",
		type: "color",
		default: DEFAULTS.text.background.color,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
	{
		key: "background.cornerRadius",
		label: "Background Radius",
		type: "number",
		default: DEFAULTS.text.background.cornerRadius,
		min: CORNER_RADIUS_MIN,
		max: CORNER_RADIUS_MAX,
		step: 1,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
	{
		key: "background.paddingX",
		label: "Background Padding X",
		type: "number",
		default: DEFAULTS.text.background.paddingX,
		min: 0,
		step: 1,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
	{
		key: "background.paddingY",
		label: "Background Padding Y",
		type: "number",
		default: DEFAULTS.text.background.paddingY,
		min: 0,
		step: 1,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
	{
		key: "background.offsetX",
		label: "Background Offset X",
		type: "number",
		default: DEFAULTS.text.background.offsetX,
		min: -100_000,
		step: 1,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
	{
		key: "background.offsetY",
		label: "Background Offset Y",
		type: "number",
		default: DEFAULTS.text.background.offsetY,
		min: -100_000,
		step: 1,
		dependencies: [{ param: "background.enabled", equals: true }],
	},
];

export const elementParamRegistry = new DefinitionRegistry<
	ElementType,
	readonly ElementParamDefinition[]
>("element params");

/** Deflicker (`@/deflicker/levels`): shown in its own section, not the params lists. */
const deflickerParamDefinitions: ElementParamDefinition[] = [
	{
		key: "deflicker.strength",
		label: "Deflicker",
		type: "select",
		default: "off",
		keyframable: false,
		options: [
			{ value: "off", label: "Off" },
			{ value: "light", label: "Light" },
			{ value: "medium", label: "Medium" },
			{ value: "strong", label: "Strong" },
		],
	},
];

elementParamRegistry.register({
	key: "video",
	definition: [...deflickerParamDefinitions, ...visualElementParams, ...animationParams({ isText: false }), ...styleParams({ isText: false }), ...zoomParams, ...cornerPinParamDefinitions, ...audioElementParams],
});
elementParamRegistry.register({
	key: "image",
	definition: [...visualElementParams, ...animationParams({ isText: false }), ...styleParams({ isText: false }), ...zoomParams, ...cornerPinParamDefinitions],
});
elementParamRegistry.register({
	key: "text",
	definition: [
		...textElementParams,
		...captionParams,
		...visualElementParams,
		...animationParams({ isText: true }), ...styleParams({ isText: true }),
	],
});
elementParamRegistry.register({
	key: "sticker",
	definition: [...visualElementParams, ...animationParams({ isText: false }), ...styleParams({ isText: false }), ...cornerPinParamDefinitions],
});
elementParamRegistry.register({
	key: "graphic",
	definition: [...visualElementParams, ...animationParams({ isText: false }), ...styleParams({ isText: false })],
});
elementParamRegistry.register({ key: "audio", definition: audioElementParams });
elementParamRegistry.register({ key: "effect", definition: [] });

export function getElementParams({
	element,
}: {
	element: TimelineElement;
}): readonly ElementParamDefinition[] {
	return elementParamRegistry.has(element.type)
		? elementParamRegistry.get(element.type)
		: [];
}

export function getBuiltInElementParams({
	type,
}: {
	type: ElementType;
}): readonly ElementParamDefinition[] {
	return elementParamRegistry.has(type) ? elementParamRegistry.get(type) : [];
}

export function getElementParam({
	element,
	key,
}: {
	element: TimelineElement;
	key: string;
}): ElementParamDefinition | null {
	return (
		getElementParams({ element }).find((param) => param.key === key) ?? null
	);
}

export function readElementParamValue({
	element,
	param,
}: {
	element: TimelineElement;
	param: ElementParamDefinition;
}): ParamValue | null {
	if (param.read) {
		return param.read({ element });
	}
	if ("params" in element) {
		return element.params[param.key] ?? param.default;
	}
	return null;
}

export function writeElementParamValue({
	element,
	param,
	value,
}: {
	element: TimelineElement;
	param: ElementParamDefinition;
	value: ParamValue;
}): TimelineElement {
	if (param.write) {
		return param.write({ element, value });
	}
	if ("params" in element) {
		return {
			...element,
			params: {
				...element.params,
				[param.key]: value,
			},
		};
	}
	return element;
}

export function buildElementParamValues({
	element,
}: {
	element: TimelineElement;
}): ParamValues {
	const values: ParamValues = {};
	for (const param of getElementParams({ element })) {
		const value = readElementParamValue({ element, param });
		if (value !== null) {
			values[param.key] = value;
		}
	}
	return values;
}


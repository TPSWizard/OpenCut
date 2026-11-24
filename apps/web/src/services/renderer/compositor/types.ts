import type { BlendMode } from "@/rendering";
import type { EffectPass } from "@/effects/types";
import type { TransitionParams } from "@/transitions/types";
import type { LayerStyleDescriptor } from "@/layer-style/style";

export type FrameDescriptor = {
	width: number;
	height: number;
	clear: {
		color: [number, number, number, number];
	};
	items: FrameItemDescriptor[];
};

export type LayerItemDescriptor = {
	type: "layer";
	textureId: string;
	transform: QuadTransformDescriptor;
	opacity: number;
	blendMode: BlendMode;
	effectPassGroups: EffectPass[][];
	mask: LayerMaskDescriptor | null;
	/** Rounded corners, outline and drop shadow; none when left out. */
	style?: LayerStyleDescriptor;
	/**
	 * The layer's corners pinned to points of its own box (top left, top
	 * right, bottom right, bottom left; box units before rotation), drawn
	 * in perspective by the shared compositor. None when left out.
	 */
	cornerPin?: [[number, number], [number, number], [number, number], [number, number]];
	/**
	 * Motion blur: the quad at shutter open and close. The shared
	 * compositor draws the layer at evenly spaced points between the two
	 * and averages them (`compositor::motion_blur_samples` decides how
	 * many). None when left out.
	 */
	motionBlur?: { from: QuadTransformDescriptor; to: QuadTransformDescriptor };
	/** Deflicker: the source's colour is multiplied by this before effects; 1 when left out. */
	brightnessGain?: number;
};

export type SceneEffectItemDescriptor = {
	type: "sceneEffect";
	effectPassGroups: EffectPass[][];
};

/**
 * Two stacks of layers blended by a gl-transitions shader (`name`) at
 * `progress` (0 shows only `from`, 1 only `to`); an empty side is
 * transparent. Mirrors `TransitionDescriptor` in `crates/compositor`.
 */
export type TransitionItemDescriptor = {
	type: "transition";
	name: string;
	progress: number;
	params: TransitionParams;
	from: LayerItemDescriptor[];
	to: LayerItemDescriptor[];
};

export type FrameItemDescriptor =
	| LayerItemDescriptor
	| SceneEffectItemDescriptor
	| TransitionItemDescriptor;

export type QuadTransformDescriptor = {
	centerX: number;
	centerY: number;
	width: number;
	height: number;
	rotationDegrees: number;
	flipX: boolean;
	flipY: boolean;
};

export type LayerMaskDescriptor = {
	textureId: string;
	feather: number;
	inverted: boolean;
};

export type TextureCanvasDrawFn = (
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
) => void;

/**
 * A layer texture whose pixels come from somewhere outside the renderer —
 * typically a decoded video/image frame or a sticker. Cached by reference
 * identity of the source object.
 */
export type ExternalTextureDescriptor = {
	kind: "external";
	id: string;
	source: CanvasImageSource;
	width: number;
	height: number;
};

/**
 * A layer texture that the renderer rasterizes from scene state (color fill,
 * text layout, mask shape, blur backdrop). Cached by `contentHash`: when it
 * matches the previous frame's hash for this id, the upload is skipped
 * entirely and the persistent canvas is not even cleared.
 */
export type RenderedTextureDescriptor = {
	kind: "rendered";
	id: string;
	contentHash: string;
	width: number;
	height: number;
	draw: TextureCanvasDrawFn;
};

export type TextureUploadDescriptor =
	| ExternalTextureDescriptor
	| RenderedTextureDescriptor;

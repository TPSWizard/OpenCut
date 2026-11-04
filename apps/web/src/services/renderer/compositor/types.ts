import type { BlendMode } from "@/rendering";
import type { EffectPass } from "@/effects/types";
import type { TransitionParams } from "@/transitions/types";

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

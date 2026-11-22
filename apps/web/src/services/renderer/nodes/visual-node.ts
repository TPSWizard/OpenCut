import { BaseNode } from "./base-node";
import type { Effect, EffectPass } from "@/effects/types";
import type { Mask } from "@/masks/types";
import type { BlendMode, Transform } from "@/rendering";
import type { RetimeConfig, VisualElement } from "@/timeline";
import type { ClipAnimations } from "@/clip-animation/presets";
import type { LayerStyle } from "@/layer-style/style";

export interface VisualNodeParams {
	duration: number;
	timeOffset: number;
	trimStart: number;
	trimEnd: number;
	retime?: RetimeConfig;
	transform: Transform;
	animations?: VisualElement["animations"];
	opacity: number;
	blendMode?: BlendMode;
	effects?: Effect[];
	masks?: Mask[];
	/** One-click in, out and loop animations, from the element's params. */
	clipAnimation?: ClipAnimations | null;
	/** Rounded corners, outline and drop shadow, from the element's params. */
	layerStyle?: LayerStyle | null;
	/**
	 * Resolve at times outside the clip too (a transition's handles): the
	 * source keeps playing from the material beyond the trim and holds its
	 * edge frame when there is none; keyframes hold their edge values.
	 */
	extended?: boolean;
	/** Times at which the clip is drawn by a transition instead. */
	suppressedWindows?: { start: number; end: number }[];
}

export interface ResolvedVisualNodeState {
	localTime: number;
	transform: Transform;
	opacity: number;
	effectPasses: EffectPass[][];
}

export interface ResolvedVisualSourceNodeState extends ResolvedVisualNodeState {
	source: CanvasImageSource;
	sourceWidth: number;
	sourceHeight: number;
}

export abstract class VisualNode<
	Params extends VisualNodeParams = VisualNodeParams,
	Resolved extends ResolvedVisualNodeState = ResolvedVisualNodeState,
> extends BaseNode<Params, Resolved> {}

import {
	VisualNode,
	type ResolvedVisualSourceNodeState,
	type VisualNodeParams,
} from "./visual-node";

/** A Lottie animation on an image element; each frame is drawn by `@/lottie`. */
export interface LottieNodeParams extends VisualNodeParams {
	mediaId: string;
	file: File;
	/** Largest side a frame is drawn at (smaller in the preview). */
	maxRasterSize?: number;
}

export class LottieNode extends VisualNode<
	LottieNodeParams,
	ResolvedVisualSourceNodeState
> {}

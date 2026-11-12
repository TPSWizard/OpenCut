import {
	VisualNode,
	type ResolvedVisualSourceNodeState,
	type VisualNodeParams,
} from "./visual-node";

export interface VideoNodeParams extends VisualNodeParams {
	url: string;
	file: File;
	mediaId: string;
	/** The preview's decoder for this source (proxy and/or scaled); see `previewVideoSource`. */
	decodeKey?: string;
	decodeSize?: { width: number; height: number };
	/** Background removal: the matte video and where it starts in the source. */
	cutout?: { matteId: string; file: File; offset: number; inverted: boolean };
}

export interface ResolvedVideoNodeState extends ResolvedVisualSourceNodeState {
	/** The cut-out's matte for this frame as alpha (white RGB), if any. */
	matte?: { canvas: OffscreenCanvas | HTMLCanvasElement; key: string };
}

export class VideoNode extends VisualNode<VideoNodeParams, ResolvedVideoNodeState> {}

import type { SceneTracks, TimelineElement, TimelineTrack, VideoElement } from "@/timeline";
import type { MediaAsset } from "@/media/types";
import { RootNode } from "./nodes/root-node";
import { VideoNode, type VideoNodeParams } from "./nodes/video-node";
import { ImageNode } from "./nodes/image-node";
import { TextNode } from "./nodes/text-node";
import { StickerNode } from "./nodes/sticker-node";
import { GraphicNode } from "./nodes/graphic-node";
import { ColorNode } from "./nodes/color-node";
import { BlurBackgroundNode } from "./nodes/blur-background-node";
import { EffectLayerNode } from "./nodes/effect-layer-node";
import { TransitionNode } from "./nodes/transition-node";
import { expandCompounds } from "@/timeline/compound";
import {
	getInTransitionWindow,
	getIncomingTransitionWindow,
	getOutgoingTransitionWindow,
	isVisualElement,
} from "@/transitions/window";
import type { AnyBaseNode } from "./nodes/base-node";
import type { TBackground, TCanvasSize } from "@/project/types";
import { DEFAULT_BACKGROUND_BLUR_INTENSITY } from "@/background/blur";
import { previewDecodeSize, type PreviewQuality } from "@/media/proxy";
import {
	buildTransformFromParams,
	readBlendModeFromParams,
	readOpacityFromParams,
} from "@/rendering";

const PREVIEW_MAX_IMAGE_SIZE = 2048;

function getVisibleSortedElements({ track }: { track: TimelineTrack }) {
	return track.elements
		.filter((element) => !("hidden" in element && element.hidden))
		.slice()
		.sort((a, b) => {
			if (a.startTime !== b.startTime) return a.startTime - b.startTime;
			return a.id.localeCompare(b.id);
		});
}

type ElementNodeOptions = {
	extended?: boolean;
	suppressedWindows?: { start: number; end: number }[];
};

/** A cut-out whose matte is in the project (a missing matte shows the clip whole). */
function cutoutParams({
	cutout,
	mediaMap,
}: {
	cutout: VideoElement["cutout"];
	mediaMap: Map<string, MediaAsset>;
}): VideoNodeParams["cutout"] {
	if (!cutout) return undefined;
	const matte = mediaMap.get(cutout.matteId);
	if (!matte?.file) return undefined;
	return { matteId: matte.id, file: matte.file, offset: cutout.offset, inverted: Boolean(cutout.inverted) };
}

/** The render node for one visual element, or null when it cannot be drawn. */
function buildElementNode({
	element,
	mediaMap,
	canvasSize,
	isPreview,
	options,
}: {
	element: TimelineElement;
	mediaMap: Map<string, MediaAsset>;
	canvasSize: TCanvasSize;
	isPreview?: boolean;
	options?: ElementNodeOptions;
}): AnyBaseNode | null {
	const extra = options ?? {};
	if (element.type === "video" || element.type === "image") {
		const mediaAsset = mediaMap.get(element.mediaId);
		if (!mediaAsset?.file || !mediaAsset?.url) {
			return null;
		}

		if (element.type === "video" && mediaAsset.type === "video") {
			return new VideoNode({
				mediaId: mediaAsset.id,
				url: mediaAsset.url,
				file: mediaAsset.file,
				duration: element.duration,
				timeOffset: element.startTime,
				trimStart: element.trimStart,
				trimEnd: element.trimEnd,
				retime: element.retime,
				transform: buildTransformFromParams({ params: element.params }),
				animations: element.animations,
				opacity: readOpacityFromParams({ params: element.params }),
				blendMode: readBlendModeFromParams({ params: element.params }),
				effects: element.effects ?? [],
				masks: element.masks ?? [],
				cutout: cutoutParams({ cutout: element.cutout, mediaMap }),
				...extra,
			});
		}
		if (element.type === "image" && mediaAsset.type === "image") {
			return new ImageNode({
				url: mediaAsset.url,
				duration: element.duration,
				timeOffset: element.startTime,
				trimStart: element.trimStart,
				trimEnd: element.trimEnd,
				transform: buildTransformFromParams({ params: element.params }),
				animations: element.animations,
				opacity: readOpacityFromParams({ params: element.params }),
				blendMode: readBlendModeFromParams({ params: element.params }),
				effects: element.effects ?? [],
				masks: element.masks ?? [],
				...(isPreview && {
					maxSourceSize: PREVIEW_MAX_IMAGE_SIZE,
				}),
				...extra,
			});
		}
		return null;
	}

	if (element.type === "text") {
		return new TextNode({
			...element,
			transform: buildTransformFromParams({ params: element.params }),
			opacity: readOpacityFromParams({ params: element.params }),
			blendMode: readBlendModeFromParams({ params: element.params }),
			canvasCenter: { x: canvasSize.width / 2, y: canvasSize.height / 2 },
			canvasHeight: canvasSize.height,
			textBaseline: "middle",
			effects: element.effects ?? [],
			...extra,
		});
	}

	if (element.type === "sticker") {
		return new StickerNode({
			stickerId: element.stickerId,
			intrinsicWidth: element.intrinsicWidth,
			intrinsicHeight: element.intrinsicHeight,
			duration: element.duration,
			timeOffset: element.startTime,
			trimStart: element.trimStart,
			trimEnd: element.trimEnd,
			transform: buildTransformFromParams({ params: element.params }),
			animations: element.animations,
			opacity: readOpacityFromParams({ params: element.params }),
			blendMode: readBlendModeFromParams({ params: element.params }),
			effects: element.effects ?? [],
			...extra,
		});
	}

	if (element.type === "graphic") {
		return new GraphicNode({
			definitionId: element.definitionId,
			params: element.params,
			duration: element.duration,
			timeOffset: element.startTime,
			trimStart: element.trimStart,
			trimEnd: element.trimEnd,
			transform: buildTransformFromParams({ params: element.params }),
			animations: element.animations,
			opacity: readOpacityFromParams({ params: element.params }),
			blendMode: readBlendModeFromParams({ params: element.params }),
			effects: element.effects ?? [],
			masks: element.masks ?? [],
			...extra,
		});
	}

	return null;
}

function buildTrackNodes({
	tracks,
	mediaMap,
	canvasSize,
	isPreview,
}: {
	tracks: TimelineTrack[];
	mediaMap: Map<string, MediaAsset>;
	canvasSize: TCanvasSize;
	isPreview?: boolean;
}): AnyBaseNode[] {
	const nodes: AnyBaseNode[] = [];

	for (const track of tracks) {
		const elements = getVisibleSortedElements({ track });

		for (const element of elements) {
			if (element.type === "effect") {
				nodes.push(
					new EffectLayerNode({
						effectType: element.effectType,
						effectParams: element.params,
						timeOffset: element.startTime,
						duration: element.duration,
					}),
				);
				continue;
			}

			// Inside a transition's window the clip is drawn by the transition
			// node (as the outgoing side of its own, or the incoming side of
			// the previous clip's), never by its regular node.
			const outgoing = getOutgoingTransitionWindow({ track, element });
			const incoming = getIncomingTransitionWindow({ track, element });
			const intro = getInTransitionWindow({ track, element });
			const suppressedWindows = [outgoing, incoming, intro]
				.filter((window): window is NonNullable<typeof window> => window !== null)
				.map((window) => ({ start: window.start, end: window.end }));

			const node = buildElementNode({
				element,
				mediaMap,
				canvasSize,
				isPreview,
				options: suppressedWindows.length > 0 ? { suppressedWindows } : undefined,
			});
			if (node) {
				nodes.push(node);
			}

			if (intro && isVisualElement(element) && element.transitionIn) {
				nodes.push(
					new TransitionNode({
						name: element.transitionIn.type,
						params: element.transitionIn.params ?? {},
						windowStart: intro.start,
						windowEnd: intro.end,
						from: null,
						to: buildElementNode({
							element,
							mediaMap,
							canvasSize,
							isPreview,
							options: { extended: true },
						}),
					}),
				);
			}

			if (outgoing && isVisualElement(element) && element.transition) {
				const next = outgoing.toElementId
					? elements.find((candidate) => candidate.id === outgoing.toElementId)
					: undefined;
				nodes.push(
					new TransitionNode({
						name: element.transition.type,
						params: element.transition.params ?? {},
						windowStart: outgoing.start,
						windowEnd: outgoing.end,
						from: buildElementNode({
							element,
							mediaMap,
							canvasSize,
							isPreview,
							options: { extended: true },
						}),
						to: next
							? buildElementNode({
									element: next,
									mediaMap,
									canvasSize,
									isPreview,
									options: { extended: true },
								})
							: null,
					}),
				);
			}
		}
	}

	return nodes;
}

function buildBlurBackgroundNodes({
	track,
	mediaMap,
	blurIntensity,
}: {
	track: TimelineTrack | undefined;
	mediaMap: Map<string, MediaAsset>;
	blurIntensity: number;
}): AnyBaseNode[] {
	if (!track) {
		return [];
	}

	const nodes: AnyBaseNode[] = [];
	const elements = getVisibleSortedElements({ track });

	for (const element of elements) {
		if (element.type !== "video" && element.type !== "image") {
			continue;
		}

		const mediaAsset = mediaMap.get(element.mediaId);
		if (
			!mediaAsset?.file ||
			!mediaAsset?.url ||
			(mediaAsset.type !== "video" && mediaAsset.type !== "image")
		) {
			continue;
		}

		nodes.push(
			new BlurBackgroundNode({
				mediaId: mediaAsset.id,
				url: mediaAsset.url,
				file: mediaAsset.file,
				mediaType: mediaAsset.type,
				duration: element.duration,
				timeOffset: element.startTime,
				trimStart: element.trimStart,
				trimEnd: element.trimEnd,
				retime: element.type === "video" ? element.retime : undefined,
				blurIntensity,
			}),
		);
	}

	return nodes;
}

export type BuildSceneParams = {
	canvasSize: TCanvasSize;
	tracks: SceneTracks;
	mediaAssets: MediaAsset[];
	duration: number;
	background: TBackground;
	isPreview?: boolean;
	/** How the preview decodes video; ignored unless `isPreview`. */
	previewSource?: PreviewSourceOptions;
};

export interface PreviewSourceOptions {
	quality: PreviewQuality;
	useProxies: boolean;
}

/**
 * Which file and decoder size the preview uses for a video: its proxy when
 * proxies are on and it has one, at the size the quality setting asks
 * for. The decoder key names the variant so export (which builds its own
 * scene without these) never reads a preview decoder.
 */
export function previewVideoSource({
	asset,
	canvasSize,
	options,
}: {
	asset: MediaAsset;
	canvasSize: TCanvasSize;
	options: PreviewSourceOptions;
}): { file: File; decodeKey: string; decodeSize?: { width: number; height: number } } | null {
	const useProxy = Boolean(options.useProxies && asset.proxyFile && asset.proxyWidth && asset.proxyHeight);
	const file = useProxy ? (asset.proxyFile as File) : asset.file;
	const sourceWidth = (useProxy ? asset.proxyWidth : asset.width) ?? 0;
	const sourceHeight = (useProxy ? asset.proxyHeight : asset.height) ?? 0;
	const size = previewDecodeSize({ sourceWidth, sourceHeight, canvas: canvasSize, quality: options.quality });
	if (!useProxy && !size) return null;
	const variant = `${useProxy ? "proxy" : "original"}@${size ? `${size.width}x${size.height}` : "native"}`;
	return { file, decodeKey: `${asset.id}|${variant}`, ...(size ? { decodeSize: size } : {}) };
}

function applyPreviewSources({
	node,
	mediaMap,
	canvasSize,
	options,
}: {
	node: AnyBaseNode | null;
	mediaMap: Map<string, MediaAsset>;
	canvasSize: TCanvasSize;
	options: PreviewSourceOptions;
}): void {
	if (!node) return;
	if (node instanceof VideoNode || (node instanceof BlurBackgroundNode && node.params.mediaType === "video")) {
		const asset = mediaMap.get(node.params.mediaId);
		const source = asset ? previewVideoSource({ asset, canvasSize, options }) : null;
		if (source) {
			node.params = { ...node.params, ...source };
		}
	}
	if (node instanceof TransitionNode) {
		applyPreviewSources({ node: node.params.from, mediaMap, canvasSize, options });
		applyPreviewSources({ node: node.params.to, mediaMap, canvasSize, options });
	}
	for (const child of node.children) {
		applyPreviewSources({ node: child, mediaMap, canvasSize, options });
	}
}

export function buildScene({
	canvasSize,
	tracks: sceneTracks,
	mediaAssets,
	duration,
	background,
	isPreview,
	previewSource,
}: BuildSceneParams) {
	// Compound clips are drawn as their children.
	const tracks = expandCompounds(sceneTracks);
	const rootNode = new RootNode({ duration });
	const mediaMap = new Map(mediaAssets.map((m) => [m.id, m]));

	const visibleTracks = [
		...tracks.overlay.filter((track) => !("hidden" in track && track.hidden)),
		...(!tracks.main.hidden ? [tracks.main] : []),
	];
	const orderedTracksBottomToTop = visibleTracks.slice().reverse();
	const mainTrack = tracks.main.hidden ? undefined : tracks.main;

	const allNodes = buildTrackNodes({
		tracks: orderedTracksBottomToTop,
		mediaMap,
		canvasSize,
		isPreview,
	});

	if (background.type === "blur") {
		const blurNodes = buildBlurBackgroundNodes({
			track: mainTrack,
			mediaMap,
			blurIntensity:
				background.blurIntensity ?? DEFAULT_BACKGROUND_BLUR_INTENSITY,
		});
		for (const node of blurNodes) {
			rootNode.add(node);
		}
	} else if (
		background.type === "color" &&
		background.color !== "transparent"
	) {
		rootNode.add(new ColorNode({ color: background.color }));
	}

	for (const node of allNodes) {
		rootNode.add(node);
	}

	if (isPreview && previewSource) {
		applyPreviewSources({ node: rootNode, mediaMap, canvasSize, options: previewSource });
	}

	return rootNode;
}

/**
 * Cover pictures in the web editor: the project re-fitted to the cover's
 * size (fill) with the title as a text clip on a new top track, drawn by the
 * export renderer at the cover frame; and a frame suggested by scoring
 * evenly spaced frames. `crates/editor/src/cover.rs` (`cover_project`) and
 * `crates/render/src/cover.rs` do the same on the desktop.
 */

import type { EditorCore } from "@/core";
import type { TProjectSettings } from "@/project/types";
import { captionMargin, resizeTracks } from "@/resize/plan";
import { CanvasRenderer } from "@/services/renderer/canvas-renderer";
import { buildScene } from "@/services/renderer/scene-builder";
import { relayoutCaptions } from "@/shorts/make";
import { buildTextElement } from "@/timeline/element-utils";
import type { SceneTracks, TextElement, TextTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { mediaTimeFromSeconds, TICKS_PER_SECOND } from "@/wasm";
import {
	COVER_SAMPLES,
	type CoverSettings,
	type CoverShape,
	coverSampleTimes,
	coverSize,
	coverTitle,
	DEFAULT_COVER,
	frameScore,
	pickCoverTime,
	type Size,
} from "./layout";

/** The project's cover settings, or the defaults when none were made. */
export function coverOf(settings: TProjectSettings): CoverSettings {
	return { ...DEFAULT_COVER, ...(settings.cover ?? {}) };
}

/** The tracks as the cover draws them at `size`. */
export function coverTracks({
	editor,
	size,
	cover,
}: {
	editor: EditorCore;
	size: Size;
	cover: CoverSettings;
}): SceneTracks {
	const project = editor.project.getActive();
	const canvas = project.settings.canvasSize;
	let tracks = editor.scenes.getActiveScene().tracks;
	if (canvas.width !== size.width || canvas.height !== size.height) {
		const sizes = new Map(editor.media.getAssets().map((asset) => [asset.id, asset]));
		tracks = relayoutCaptions({
			tracks: resizeTracks({
				tracks,
				from: canvas,
				to: size,
				mode: "fill",
				sizeOf: (mediaId) => {
					const asset = sizes.get(mediaId);
					return asset?.width && asset.height ? { width: asset.width, height: asset.height } : null;
				},
			}),
			canvas: size,
			margin: captionMargin(size),
		});
	}
	const layout = coverTitle({ ...size, style: cover.style, title: cover.title });
	if (layout.lines.length === 0) return tracks;
	const start = Math.max(0, cover.time - 0.5);
	const title = {
		...buildTextElement({
			raw: {
				name: "Cover title",
				duration: mediaTimeFromSeconds({ seconds: cover.time + 1 - start }),
				params: {
					content: layout.lines.join("\n"),
					fontSize: layout.fontSize,
					fontFamily: "Arial",
					fontWeight: "bold",
					textAlign: "center",
					lineHeight: 1.2,
					color: layout.color,
					"transform.positionX": 0,
					"transform.positionY": layout.positionY,
					...(layout.outlineWidth > 0 ? { "outline.width": layout.outlineWidth, "outline.color": "#000000" } : {}),
					...(layout.background
						? {
								"background.enabled": true,
								"background.color": layout.background.color,
								"background.paddingX": layout.background.paddingX,
								"background.paddingY": layout.background.paddingY,
								"background.cornerRadius": layout.background.cornerRadius,
							}
						: {}),
				},
			},
			startTime: mediaTimeFromSeconds({ seconds: start }),
		}),
		id: generateUUID(),
	} as TextElement;
	const track: TextTrack = { id: generateUUID(), name: "Cover title", type: "text", elements: [title], hidden: false };
	return { ...tracks, overlay: [track, ...tracks.overlay] };
}

async function renderTracks({
	editor,
	tracks,
	size,
	seconds,
}: {
	editor: EditorCore;
	tracks: SceneTracks;
	size: Size;
	seconds: number;
}): Promise<HTMLCanvasElement> {
	const project = editor.project.getActive();
	const duration = editor.timeline.getTotalDuration();
	const node = buildScene({
		tracks,
		mediaAssets: editor.media.getAssets(),
		duration,
		canvasSize: size,
		background: project.settings.background,
	});
	const canvas = document.createElement("canvas");
	canvas.width = size.width;
	canvas.height = size.height;
	const fps = project.settings.fps;
	const lastFrame = Math.max(0, duration / TICKS_PER_SECOND - fps.denominator / fps.numerator);
	const renderer = new CanvasRenderer({ width: size.width, height: size.height, fps });
	try {
		await renderer.renderToCanvas({ node, time: mediaTimeFromSeconds({ seconds: Math.min(Math.max(0, seconds), lastFrame) }), targetCanvas: canvas });
	} finally {
		// The compositor canvas is the preview's too: draw the preview again.
		editor.renderer.requestRedraw();
	}
	return canvas;
}

/** The cover as a PNG of `shape` (default: the canvas shape). */
export async function renderCoverPng({
	editor,
	cover = coverOf(editor.project.getActive().settings),
	shape = "canvas",
}: {
	editor: EditorCore;
	cover?: CoverSettings;
	shape?: CoverShape;
}): Promise<{ blob: Blob; width: number; height: number }> {
	const size = coverSize(shape, editor.project.getActive().settings.canvasSize);
	const canvas = await renderTracks({ editor, tracks: coverTracks({ editor, size, cover }), size, seconds: cover.time });
	const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
	if (!blob) throw new Error("The cover could not be saved");
	return { blob, ...size };
}

/** Width the frames are scored at. */
const SCORE_WIDTH = 160;

/** The timeline seconds of the best-looking frame among evenly spaced samples. */
export async function suggestCoverTime({ editor }: { editor: EditorCore }): Promise<number> {
	const duration = editor.timeline.getTotalDuration() / TICKS_PER_SECOND;
	if (duration <= 0) throw new Error("The timeline is empty");
	const canvas = editor.project.getActive().settings.canvasSize;
	const size = { width: SCORE_WIDTH, height: Math.max(2, Math.round((SCORE_WIDTH * canvas.height) / canvas.width)) };
	// Rendered at the canvas's shape, scaled down: the frame as it plays.
	const tracks = editor.scenes.getActiveScene().tracks;
	const samples: { time: number; score: number }[] = [];
	for (const time of coverSampleTimes(duration, COVER_SAMPLES)) {
		const frame = await renderTracks({ editor, tracks: scaleTracks({ editor, tracks, size }), size, seconds: time });
		const context = frame.getContext("2d");
		if (!context) continue;
		samples.push({ time, score: frameScore(context.getImageData(0, 0, size.width, size.height).data, size.width, size.height).score });
	}
	return pickCoverTime(samples, duration);
}

/** The tracks scaled to a smaller frame of the same shape (keep the picture, smaller pixels). */
function scaleTracks({ editor, tracks, size }: { editor: EditorCore; tracks: SceneTracks; size: Size }): SceneTracks {
	const canvas = editor.project.getActive().settings.canvasSize;
	const sizes = new Map(editor.media.getAssets().map((asset) => [asset.id, asset]));
	return resizeTracks({
		tracks,
		from: canvas,
		to: size,
		mode: "fit",
		sizeOf: (mediaId) => {
			const asset = sizes.get(mediaId);
			return asset?.width && asset.height ? { width: asset.width, height: asset.height } : null;
		},
	});
}

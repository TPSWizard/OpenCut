"use client";

import { useRef } from "react";
import { getElementLocalTime, resolveAnimationPathValueAtTime } from "@/animation";
import type { ElementAnimations } from "@/animation/types";
import { canCornerPin, cornerPinUpdates } from "@/corner-pin/apply";
import { type CornerPin, PIN_CORNERS, boxToCanvas, canvasToBox, cornerPinCorners, isConvexPin } from "@/corner-pin/pin";
import { useEditor } from "@/editor/use-editor";
import { usePreviewViewport } from "@/preview/components/preview-viewport";
import { getVisibleElementsWithBounds } from "@/preview/element-bounds";
import type { TimelineElement } from "@/timeline";
import { mediaTime } from "@/wasm";
import { CornerHandle } from "./handle-primitives";

interface PinDrag {
	index: number;
	trackId: string;
	element: TimelineElement;
	corners: CornerPin;
	moved: boolean;
}

/**
 * The four corner pin handles of the selected video, picture or sticker
 * clip, shown while its Corner pin tab is open. Dragging one pins that
 * corner (switching the pin on); a drag that would cross or fold the
 * corners in is refused.
 */
export function CornerPinHandles() {
	const viewport = usePreviewViewport();
	const editor = useEditor();
	const selected = useEditor((e) => e.selection.getSelectedElements());
	const tracks = useEditor((e) => e.timeline.getPreviewTracks() ?? e.scenes.getActiveScene().tracks);
	const currentTime = useEditor((e) => e.playback.getCurrentTime());
	const mediaAssets = useEditor((e) => e.media.getAssets());
	const canvasSize = useEditor((e) => e.project.getActive().settings.canvasSize);
	const drag = useRef<PinDrag | null>(null);

	if (selected.length !== 1) return null;
	const target = getVisibleElementsWithBounds({ tracks, currentTime, canvasSize, mediaAssets }).find(
		(item) => item.elementId === selected[0].elementId && item.trackId === selected[0].trackId,
	);
	if (!target || !canCornerPin(target.element) || !("params" in target.element)) return null;
	const { element, bounds } = target;
	const params = element.params;
	const animations = "animations" in element ? (element.animations as ElementAnimations | undefined) : undefined;
	const localTime = getElementLocalTime({
		timelineTime: currentTime,
		elementStartTime: element.startTime,
		elementDuration: element.duration,
	});
	const corners = cornerPinCorners((key) =>
		resolveAnimationPathValueAtTime({
			animations,
			propertyPath: key,
			localTime,
			fallbackValue: typeof params[key] === "number" ? (params[key] as number) : Number.NaN,
		}),
	);
	const screens = corners.map((corner) => {
		const [canvasX, canvasY] = boxToCanvas(bounds, corner);
		return viewport.canvasToOverlay({ canvasX, canvasY });
	});

	const onPointerDown = (index: number) => (event: React.PointerEvent) => {
		if (viewport.handlePanPointerDown({ event })) return;
		event.stopPropagation();
		(event.currentTarget as Element).setPointerCapture?.(event.pointerId);
		drag.current = { index, trackId: target.trackId, element, corners, moved: false };
	};
	const onPointerMove = (event: React.PointerEvent) => {
		if (viewport.handlePanPointerMove({ event })) return;
		const session = drag.current;
		if (!session) return;
		const point = viewport.screenToCanvas({ clientX: event.clientX, clientY: event.clientY });
		if (!point) return;
		const next = session.corners.map((corner) => [...corner]) as CornerPin;
		next[session.index] = canvasToBox(bounds, [point.x, point.y]);
		if (!isConvexPin(next)) return;
		session.moved = true;
		editor.timeline.previewElements({
			updates: [
				{
					trackId: session.trackId,
					elementId: session.element.id,
					updates: cornerPinUpdates({ element: session.element, corners: next, localTime: mediaTime({ ticks: localTime }) }),
				},
			],
		});
	};
	const onPointerUp = (event: React.PointerEvent) => {
		if (viewport.handlePanPointerUp({ event })) return;
		const session = drag.current;
		drag.current = null;
		if (!session) return;
		if (session.moved) editor.timeline.commitPreview();
		else editor.timeline.discardPreview();
	};

	const outline = screens.map((screen) => `${screen.x},${screen.y}`).join(" ");
	return (
		<div className="pointer-events-none absolute inset-0 overflow-hidden" data-testid="corner-pin-handles" aria-hidden>
			<svg className="absolute inset-0 size-full" role="presentation">
				<polygon points={outline} fill="none" stroke="white" strokeWidth={1.5} strokeOpacity={0.9} />
				<polygon points={outline} fill="none" stroke="black" strokeWidth={1.5} strokeOpacity={0.5} strokeDasharray="4 4" />
			</svg>
			{screens.map((screen, index) => (
				<div key={PIN_CORNERS[index].corner} data-testid={`corner-pin-handle-${PIN_CORNERS[index].corner}`}>
					<CornerHandle
						cursor="crosshair"
						screen={screen}
						onPointerDown={onPointerDown(index)}
						onPointerMove={onPointerMove}
						onPointerUp={onPointerUp}
					/>
				</div>
			))}
		</div>
	);
}

import { drawingAddStroke, drawingRemoveLastStroke, drawingStrokeCount } from "opencut-wasm";
import { create } from "zustand";
import type { EditorCore } from "@/core";
import { DRAWING_DEFINITION } from "@/graphics/definitions/drawing";
import { buildGraphicElement } from "@/timeline/element-utils";
import type { GraphicElement, TimelineElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";

/** What a stroke is drawn with (`strokes::Tool`). */
export const DRAW_TOOLS = ["pen", "marker", "highlighter"] as const;
export type DrawTool = (typeof DRAW_TOOLS)[number];

/** A new stroke's width for each tool, a share of the canvas height (`Tool::default_width`). */
export const DRAW_TOOL_WIDTHS: Record<DrawTool, number> = { pen: 0.006, marker: 0.012, highlighter: 0.035 };
/** How opaque each tool's ink is (`Tool::opacity`). */
export const DRAW_TOOL_OPACITY: Record<DrawTool, number> = { pen: 1, marker: 1, highlighter: 0.4 };
/** The colours offered (the desktop offers the same). */
export const DRAW_COLOURS = ["#ffffff", "#000000", "#ff3b30", "#ffeb3b", "#34c759", "#0a84ff"] as const;

export interface DrawBrush {
	tool: DrawTool;
	color: string;
	/** Share of the canvas height. */
	width: number;
}

export function brushFor(tool: DrawTool, color?: string): DrawBrush {
	return { tool, color: color ?? (tool === "highlighter" ? "#ffeb3b" : "#ff3b30"), width: DRAW_TOOL_WIDTHS[tool] };
}

/** A point: canvas units across and down, pressure 0 to 1, seconds since the stroke began. */
export type StrokePoint = [number, number, number, number];

export interface StrokeInput extends DrawBrush {
	points: StrokePoint[];
}

interface DrawState {
	/** The brush while Draw is on; null when it is off. */
	brush: DrawBrush | null;
	setBrush: (brush: DrawBrush | null) => void;
}

export const useDrawStore = create<DrawState>()((set) => ({
	brush: null,
	setBrush: (brush) => set({ brush }),
}));

export function isDrawing(element: TimelineElement | undefined | null): element is GraphicElement {
	return element?.type === "graphic" && element.definitionId === DRAWING_DEFINITION;
}

export function strokeCount(element: GraphicElement): number {
	return drawingStrokeCount(typeof element.params.strokes === "string" ? element.params.strokes : "");
}

function findElement(editor: EditorCore, trackId: string, elementId: string): TimelineElement | undefined {
	const tracks = editor.scenes.getActiveScene().tracks;
	const all = [...tracks.overlay, tracks.main, ...tracks.audio];
	return all.find((track) => track.id === trackId)?.elements.find((element) => element.id === elementId);
}

/**
 * Adds a stroke: onto the selected drawing when it shows at the playhead,
 * else to a new drawing there, which is then selected (one undo step).
 * The same as the desktop's `editor::add_stroke_command`. Returns the
 * drawing's track and id.
 */
export function addDrawingStroke({
	editor,
	stroke,
}: {
	editor: EditorCore;
	stroke: StrokeInput;
}): { trackId: string; elementId: string } {
	if (stroke.points.length === 0) throw new Error("A stroke needs at least one point");
	const strokeJson = JSON.stringify({ tool: stroke.tool, color: stroke.color, width: stroke.width, points: stroke.points });
	const playhead = editor.playback.getCurrentTime();
	const selected = editor.selection.getSelectedElements();
	if (selected.length === 1) {
		const [ref] = selected;
		const element = findElement(editor, ref.trackId, ref.elementId);
		if (isDrawing(element) && element.startTime <= playhead && playhead < element.startTime + element.duration) {
			const strokes = drawingAddStroke(typeof element.params.strokes === "string" ? element.params.strokes : "", strokeJson);
			editor.timeline.updateElements({
				updates: [{ trackId: ref.trackId, elementId: ref.elementId, patch: { params: { ...element.params, strokes } } }],
			});
			return { trackId: ref.trackId, elementId: ref.elementId };
		}
	}
	const strokes = drawingAddStroke("", strokeJson);
	const element = buildGraphicElement({ definitionId: DRAWING_DEFINITION, name: "Drawing", startTime: playhead, params: { strokes } });
	editor.timeline.insertElement({ element, placement: { mode: "auto", trackType: "graphic" } });
	// The new element is the drawing at the playhead with exactly these strokes.
	const tracks = editor.scenes.getActiveScene().tracks;
	for (const track of [...tracks.overlay, tracks.main]) {
		const found = track.elements.find((candidate) => isDrawing(candidate) && candidate.params.strokes === strokes && candidate.startTime === playhead);
		if (found) {
			editor.selection.setSelectedElements({ elements: [{ trackId: track.id, elementId: found.id }] });
			return { trackId: track.id, elementId: found.id };
		}
	}
	throw new Error("The drawing could not be added");
}

/** Takes a drawing's last stroke away (one undo step). */
export function removeLastStroke({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): void {
	const element = findElement(editor, trackId, elementId);
	if (!isDrawing(element)) throw new Error("The element is not a drawing");
	const strokes = drawingRemoveLastStroke(typeof element.params.strokes === "string" ? element.params.strokes : "");
	editor.timeline.updateElements({ updates: [{ trackId, elementId, patch: { params: { ...element.params, strokes } } }] });
}

/** Seconds since `start` (performance clock, ms). */
export function secondsSince(start: number, now: number): number {
	return Math.max(0, (now - start) / 1000);
}

export const TICKS = TICKS_PER_SECOND;

/**
 * Slip, slide and roll (row 93): three timeline tools beside the usual
 * select-and-move. The edit itself is the shared `trimedit` crate (WASM
 * `trimEditPlan`), the same as the desktop: slip moves what a clip shows
 * inside it, slide moves a clip between its neighbours (they give and
 * take), roll moves the cut between two touching clips. A drag previews
 * the edit and commits it as one undo step on release.
 */

import { trimEditPlan } from "opencut-wasm";
import { create } from "zustand";
import type { EditorCore } from "@/core";
import type { TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";

export const TRIM_TOOLS = ["select", "slip", "slide", "roll"] as const;
export type TrimToolMode = (typeof TRIM_TOOLS)[number];
export type TrimEditTool = Exclude<TrimToolMode, "select">;

interface TrimToolState {
	tool: TrimToolMode;
	setTool: (tool: TrimToolMode) => void;
}

export const useTrimToolStore = create<TrimToolState>()((set) => ({
	tool: "select",
	setTool: (tool) => set({ tool }),
}));

export interface TrimChange {
	id: string;
	start: number;
	duration: number;
	trimStart: number;
	trimEnd: number;
}

export interface TrimPlan {
	/** The move that fitted (ticks). */
	delta: number;
	changes: TrimChange[];
}

function asClip(element: TimelineElement) {
	const retime = (element as { retime?: { rate?: number; ramp?: unknown[]; reverse?: boolean } }).retime;
	return {
		id: element.id,
		start: element.startTime,
		duration: element.duration,
		trimStart: element.trimStart,
		trimEnd: element.trimEnd,
		limited: element.type === "video" || element.type === "audio",
		retimed: Boolean(retime && ((retime.rate ?? 1) !== 1 || (retime.ramp?.length ?? 0) > 0 || retime.reverse)),
	};
}

/** One frame at the project's rate, in ticks (the shortest a clip may become). */
export function minDurationTicks(editor: EditorCore): number {
	const fps = editor.project.getActive().settings.fps;
	return Math.max(1, Math.round((TICKS_PER_SECOND * fps.denominator) / fps.numerator));
}

/** The edit a tool makes on `elementId` of `track` moved by `deltaTicks`; throws why it cannot. */
export function planTrimEdit({
	tool,
	track,
	elementId,
	deltaTicks,
	minDuration,
}: {
	tool: TrimEditTool;
	track: TimelineTrack;
	elementId: string;
	deltaTicks: number;
	minDuration: number;
}): TrimPlan {
	const clips = (track.elements as TimelineElement[]).map(asClip);
	return JSON.parse(trimEditPlan(tool, elementId, JSON.stringify(clips), deltaTicks, minDuration)) as TrimPlan;
}

function changesAsUpdates(trackId: string, plan: TrimPlan) {
	return plan.changes.map((change) => ({
		trackId,
		elementId: change.id,
		updates: {
			startTime: change.start,
			duration: change.duration,
			trimStart: change.trimStart,
			trimEnd: change.trimEnd,
		} as Partial<TimelineElement>,
	}));
}

function findTrack(editor: EditorCore, trackId: string): TimelineTrack {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	if (!track) throw new Error("The track is not on the timeline");
	if (track.locked) throw new Error("The clip's track is locked");
	return track;
}

/** Makes a trim edit now, in one undo step. Returns the move that fitted. */
export function applyTrimEdit({
	editor,
	tool,
	trackId,
	elementId,
	deltaTicks,
}: {
	editor: EditorCore;
	tool: TrimEditTool;
	trackId: string;
	elementId: string;
	deltaTicks: number;
}): TrimPlan {
	const plan = planTrimEdit({ tool, track: findTrack(editor, trackId), elementId, deltaTicks, minDuration: minDurationTicks(editor) });
	if (plan.delta !== 0) {
		editor.timeline.previewElements({ updates: changesAsUpdates(trackId, plan) });
		editor.timeline.commitPreview();
	}
	return plan;
}

/**
 * The clip whose end a roll moves when a clip is grabbed in roll mode: the
 * clip itself when grabbed in its later half, else the clip that ends
 * where it starts (if one touches it).
 */
export function rollTarget({ track, element, laterHalf }: { track: TimelineTrack; element: TimelineElement; laterHalf: boolean }): string {
	if (laterHalf) return element.id;
	const touching = (track.elements as TimelineElement[]).find(
		(other) => other.id !== element.id && Math.abs(other.startTime + other.duration - element.startTime) <= 1200,
	);
	return touching?.id ?? element.id;
}

/**
 * A drag with a trim tool: pointer moves preview the edit (dragging right
 * slides or rolls later; for slip the picture moves with the pointer, so
 * dragging right shows earlier media), release commits it.
 */
export function startTrimDrag({
	editor,
	tool,
	track,
	element,
	startX,
	laterHalf,
	pixelsPerSecond,
	onStatus,
}: {
	editor: EditorCore;
	tool: TrimEditTool;
	track: TimelineTrack;
	element: TimelineElement;
	startX: number;
	laterHalf: boolean;
	pixelsPerSecond: number;
	onStatus?: (message: string | null) => void;
}): () => void {
	const target = tool === "roll" ? rollTarget({ track, element, laterHalf }) : element.id;
	const minDuration = minDurationTicks(editor);
	const fps = editor.project.getActive().settings.fps;
	const frame = (TICKS_PER_SECOND * fps.denominator) / fps.numerator;
	const deltaAt = (clientX: number) => {
		const seconds = (clientX - startX) / Math.max(pixelsPerSecond, 1e-6);
		const ticks = Math.round((seconds * TICKS_PER_SECOND) / frame) * frame;
		return Math.round(tool === "slip" ? -ticks : ticks);
	};
	const move = (event: PointerEvent | MouseEvent) => {
		try {
			editor.timeline.discardPreview();
			const plan = planTrimEdit({ tool, track, elementId: target, deltaTicks: deltaAt(event.clientX), minDuration });
			editor.timeline.previewElements({ updates: changesAsUpdates(track.id, plan) });
			onStatus?.(null);
		} catch (error) {
			onStatus?.(error instanceof Error ? error.message : String(error));
		}
	};
	const up = () => {
		window.removeEventListener("pointermove", move);
		window.removeEventListener("pointerup", up);
		window.removeEventListener("mousemove", move);
		window.removeEventListener("mouseup", up);
		editor.timeline.commitPreview();
	};
	window.addEventListener("pointermove", move);
	window.addEventListener("pointerup", up);
	window.addEventListener("mousemove", move);
	window.addEventListener("mouseup", up);
	return up;
}

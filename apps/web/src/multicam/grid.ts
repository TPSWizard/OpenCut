/**
 * The multicam grid (row 102): every angle of a group drawn small at once
 * in the preview, the live one framed, so the editor can watch them all
 * and cut to one with keys 1-9 (or a click) at the playhead while it
 * plays. The cells, the clips' placements and the live angle come from the
 * shared Rust `multicam` crate through WASM; the desktop draws the same
 * grid (`editor::multicam_grid_project`). Checked against
 * `fixtures/multicam/grid.json`.
 */

import { multicamGrid, multicamGridCellAt, multicamGridMaxAngles, multicamLiveAngle } from "opencut-wasm";
import { create } from "zustand";
import type { ElementAnimations } from "@/animation/types";
import type { EditorCore } from "@/core";
import type { MediaAsset } from "@/media/types";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { groupCuts, multicamGroup, multicamSwitchCommand, type MulticamAngle } from "./group";

export interface GridCell {
	/** Canvas pixels from the top left. */
	x: number;
	y: number;
	width: number;
	height: number;
	positionX: number;
	positionY: number;
	scale: number;
}

/** The grid's cells and placements for angles of `sources` sizes on a canvas. */
export function gridCells(canvas: { width: number; height: number }, sources: { width: number; height: number }[]): GridCell[] {
	const flat = multicamGrid(canvas.width, canvas.height, Float64Array.from(sources.flatMap((source) => [source.width, source.height])));
	const cells: GridCell[] = [];
	for (let index = 0; index + 6 < flat.length; index += 7) {
		cells.push({
			x: flat[index],
			y: flat[index + 1],
			width: flat[index + 2],
			height: flat[index + 3],
			positionX: flat[index + 4],
			positionY: flat[index + 5],
			scale: flat[index + 6],
		});
	}
	return cells;
}

/** The angle whose cell holds canvas point (x, y), or null. */
export function gridCellAt(count: number, canvas: { width: number; height: number }, x: number, y: number): number | null {
	const index = multicamGridCellAt(count, canvas.width, canvas.height, x, y);
	return index < 0 ? null : index;
}

/** The angle the group shows at timeline second `time`. */
export function liveAngle(angles: MulticamAngle[], time: number): number {
	return multicamLiveAngle(groupCuts(angles), time);
}

export const GRID_MAX_ANGLES = (() => {
	try {
		return multicamGridMaxAngles();
	} catch {
		return 16;
	}
})();

const DROPPED_PATHS = ["opacity", "transform.positionX", "transform.positionY", "transform.scaleX", "transform.scaleY", "transform.rotate"];

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** The group's angles (at most the grid's), each with its source size. */
export function gridAngles(
	tracks: SceneTracks,
	elementId: string,
	assets: readonly MediaAsset[],
	canvas: { width: number; height: number },
): { angles: MulticamAngle[]; sources: { width: number; height: number }[] } | null {
	const group = multicamGroup(tracks, elementId);
	if (!group) return null;
	const angles = group.angles.slice(0, GRID_MAX_ANGLES);
	const sources = angles.map(({ element }) => {
		const asset = assets.find((candidate) => candidate.id === element.mediaId);
		return asset?.width && asset.height ? { width: asset.width, height: asset.height } : canvas;
	});
	return { angles, sources };
}

/**
 * The tracks the preview draws for the grid: only the group's angles, each
 * fitted into its cell, fully opaque and blended normally (its opacity and
 * transform keys dropped). Null when `elementId` is not in a group.
 */
export function multicamGridTracks({
	tracks,
	elementId,
	assets,
	canvas,
}: {
	tracks: SceneTracks;
	elementId: string;
	assets: readonly MediaAsset[];
	canvas: { width: number; height: number };
}): SceneTracks | null {
	const found = gridAngles(tracks, elementId, assets, canvas);
	if (!found) return null;
	const cells = gridCells(canvas, found.sources);
	const placed = new Map(found.angles.map((angle, index) => [angle.element.id, cells[index]]));
	const rewrite = <T extends TimelineTrack>(track: T): T => {
		const elements = (track.elements as TimelineElement[]).flatMap((element) => {
			const cell = placed.get(element.id);
			if (!cell) return [];
			const animations: ElementAnimations = { ...(element.animations ?? {}) };
			for (const path of DROPPED_PATHS) delete animations[path as keyof ElementAnimations];
			return [
				{
					...element,
					animations,
					params: {
						...element.params,
						opacity: 1,
						blendMode: "normal",
						"transform.positionX": cell.positionX,
						"transform.positionY": cell.positionY,
						"transform.scaleX": cell.scale,
						"transform.scaleY": cell.scale,
						"transform.rotate": 0,
					},
				} as TimelineElement,
			];
		});
		return { ...track, elements } as T;
	};
	return { overlay: tracks.overlay.map(rewrite), main: rewrite(tracks.main), audio: tracks.audio.map(rewrite) };
}

/** Which group the grid shows (an angle's element id), or null when it is closed. */
export const useMulticamGridStore = create<{ elementId: string | null; setElementId: (elementId: string | null) => void }>((set) => ({
	elementId: null,
	setElementId: (elementId) => set({ elementId }),
}));

/**
 * The group the grid and keys 1-9 work on: the grid's while it is open,
 * else the selected clip's, else the first group on the timeline.
 */
export function multicamTarget(editor: EditorCore): string | null {
	const tracks = editor.scenes.getActiveScene().tracks;
	const open = useMulticamGridStore.getState().elementId;
	if (open && multicamGroup(tracks, open)) return open;
	for (const { elementId } of editor.selection.getSelectedElements()) {
		if (multicamGroup(tracks, elementId)) return elementId;
	}
	for (const track of allTracks(tracks)) {
		for (const element of track.elements as TimelineElement[]) {
			if (multicamGroup(tracks, element.id)) return element.id;
		}
	}
	return null;
}

/** Cuts the target group to `angle` (0-based) at the playhead, playing or not; one undo step. */
export function switchToAngle(editor: EditorCore, angle: number): void {
	const elementId = multicamTarget(editor);
	if (!elementId) throw new Error("Sync two or more angles by audio first; the grid shows a multicam group");
	const time = editor.playback.getCurrentTime() / TICKS_PER_SECOND;
	editor.command.execute({
		command: multicamSwitchCommand({ tracks: editor.scenes.getActiveScene().tracks, elementId, time, angle }),
	});
}

/** Opens the grid on the target group, or closes it; returns whether it is open. */
export function toggleMulticamGrid(editor: EditorCore, open?: boolean): boolean {
	const store = useMulticamGridStore.getState();
	const wanted = open ?? store.elementId === null;
	if (!wanted) {
		store.setElementId(null);
		return false;
	}
	const target = multicamTarget(editor);
	if (!target) throw new Error("Sync two or more angles by audio first; the grid shows a multicam group");
	store.setElementId(target);
	return true;
}

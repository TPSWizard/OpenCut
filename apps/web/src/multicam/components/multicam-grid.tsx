"use client";

/**
 * The multicam grid over the preview (row 102): each angle's number and
 * name in its cell, the live angle framed (red while playing, like a
 * tally light), a click cutting to that angle at the playhead; and keys
 * 1-9 cutting the group to that angle whether the grid is open or not.
 */

import { useEffect, useState } from "react";
import { GridViewIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { useKeybindingsStore } from "@/actions/keybindings-store";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { usePreviewViewport } from "@/preview/components/preview-viewport";
import type { TimelineElement } from "@/timeline/types";
import { isTypableDOMElement } from "@/utils/browser";
import { TICKS_PER_SECOND } from "@/wasm";
import { MULTICAM_GROUP } from "../group";
import { gridAngles, gridCells, liveAngle, multicamTarget, switchToAngle, toggleMulticamGrid, useMulticamGridStore } from "../grid";

function useHasGroup(): boolean {
	return useEditor((e) => {
		const tracks = e.scenes.getActiveScene().tracks;
		return [...tracks.overlay, tracks.main, ...tracks.audio].some((track) =>
			(track.elements as TimelineElement[]).some((element) => {
				const group = "params" in element ? element.params[MULTICAM_GROUP] : undefined;
				return typeof group === "string" && group.length > 0;
			}),
		);
	});
}

function usePlayhead(): { seconds: number; playing: boolean } {
	const editor = useEditor();
	const [state, setState] = useState(() => ({
		seconds: editor.playback.getCurrentTime() / TICKS_PER_SECOND,
		playing: editor.playback.getIsPlaying(),
	}));
	useEffect(() => {
		const update = () =>
			setState((previous) => {
				const next = { seconds: editor.playback.getCurrentTime() / TICKS_PER_SECOND, playing: editor.playback.getIsPlaying() };
				return previous.seconds === next.seconds && previous.playing === next.playing ? previous : next;
			});
		const unsubscribes = [editor.playback.onUpdate(update), editor.playback.onSeek(update), editor.playback.subscribe(update)];
		return () => {
			for (const unsubscribe of unsubscribes) unsubscribe();
		};
	}, [editor]);
	return state;
}

/** The toolbar button that opens and closes the grid; shown when the timeline has a multicam group. */
export function MulticamGridButton() {
	const t = useT();
	const editor = useEditor();
	const hasGroup = useHasGroup();
	const open = useMulticamGridStore((state) => state.elementId !== null);
	if (!hasGroup && !open) return null;
	return (
		<Button
			variant={open ? "secondary" : "text"}
			size="icon"
			aria-label={t("Multicam grid")}
			title={t("Multicam grid: every angle at once; press 1-9 to cut to one while playing")}
			data-testid="multicam-grid-toggle"
			onClick={() => {
				try {
					toggleMulticamGrid(editor);
				} catch {
					// No group to show.
				}
			}}
		>
			<HugeiconsIcon icon={GridViewIcon} />
		</Button>
	);
}

/** The labels, live frame and click targets over the grid the preview draws. */
export function MulticamGridOverlay() {
	const t = useT();
	const editor = useEditor();
	const viewport = usePreviewViewport();
	const elementId = useMulticamGridStore((state) => state.elementId);
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const assets = useEditor((e) => e.media.getAssets());
	const canvas = useEditor((e) => e.project.getActive()?.settings.canvasSize);
	const { seconds, playing } = usePlayhead();
	const [error, setError] = useState<string | null>(null);

	const found = elementId && canvas ? gridAngles(tracks, elementId, assets, canvas) : null;
	// The group went away (ungrouped, undone, deleted): close the grid.
	useEffect(() => {
		if (elementId && !found) useMulticamGridStore.getState().setElementId(null);
	}, [elementId, found]);
	if (!found || !canvas) return null;

	const cells = gridCells(canvas, found.sources);
	const live = liveAngle(found.angles, seconds);
	const scale = viewport.sceneWidth / canvas.width;

	return (
		<div
			className="absolute"
			style={{ left: viewport.sceneLeft, top: viewport.sceneTop, width: viewport.sceneWidth, height: viewport.sceneHeight }}
			data-testid="multicam-grid"
			data-live={live}
		>
			{found.angles.map((angle, index) => {
				const cell = cells[index];
				if (!cell) return null;
				const isLive = angle.angle === live;
				return (
					<button
						type="button"
						key={angle.element.id}
						className="absolute cursor-pointer text-left"
						style={{
							left: cell.x * scale,
							top: cell.y * scale,
							width: cell.width * scale,
							height: cell.height * scale,
							outline: isLive ? `3px solid ${playing ? "#ef4444" : "#ffffff"}` : "none",
							outlineOffset: 2,
						}}
						title={t("Cut to {name} at the playhead", { name: angle.element.name })}
						data-testid={`multicam-grid-cell-${angle.angle + 1}`}
						data-live={isLive || undefined}
						onClick={() => {
							try {
								switchToAngle(editor, angle.angle);
								setError(null);
							} catch (thrown) {
								setError(thrown instanceof Error ? thrown.message : String(thrown));
							}
						}}
					>
						<span
							className="absolute left-1 top-1 rounded px-1.5 text-xs text-white"
							style={{ background: isLive ? "rgba(220, 38, 38, 0.9)" : "rgba(0, 0, 0, 0.6)" }}
						>
							{angle.angle < 9 ? `${angle.angle + 1}  ` : ""}
							{angle.element.name}
						</span>
					</button>
				);
			})}
			{error && (
				<p className="absolute bottom-1 left-1 rounded bg-black/70 px-2 text-xs text-white" role="status">
					{error}
				</p>
			)}
		</div>
	);
}

/**
 * Keys 1-9 cut the multicam group to that angle at the playhead, also
 * while playing; nothing happens without a group, while typing, or when
 * the key is bound to something else.
 */
export function MulticamKeys() {
	const editor = useEditor();
	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.repeat) return;
			const match = /^Digit([1-9])$/.exec(event.code) ?? /^Numpad([1-9])$/.exec(event.code);
			if (!match) return;
			if (event.target instanceof HTMLElement && isTypableDOMElement({ element: event.target })) return;
			if (useKeybindingsStore.getState().keybindings.has(match[1] as never)) return;
			if (!multicamTarget(editor)) return;
			event.preventDefault();
			try {
				switchToAngle(editor, Number(match[1]) - 1);
			} catch {
				// The group has no such angle.
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [editor]);
	return null;
}

"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { formatTimecode } from "opencut-wasm";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { useEditor } from "@/editor/use-editor";
import { DEFAULT_TIMELINE_BOOKMARK_COLOR } from "@/timeline/components/theme";
import type { Bookmark } from "@/timeline/types";
import { DEFAULT_FPS } from "@/fps/defaults";
import { TICKS_PER_SECOND, type MediaTime } from "@/wasm";
import { chaptersText } from "../chapters";
import { seekToBookmarkTime } from "./bookmarks";
import { useT } from "@/i18n";

/** The active scene's bookmarks as a list: go to, colour, note, length, remove. */
export function MarkersView() {
	const t = useT();
	const editor = useEditor();
	const bookmarks = useEditor((e) => e.scenes.getActiveSceneOrNull()?.bookmarks ?? []);
	const fps = useEditor((e) => e.project.getActive()?.settings.fps ?? DEFAULT_FPS);

	const addAtPlayhead = () => {
		const time = editor.playback.getCurrentTime();
		if (editor.scenes.isBookmarked({ time })) {
			toast.info(t("There is already a marker at the playhead"));
			return;
		}
		void editor.scenes.toggleBookmark({ time });
	};

	const copyChapters = async () => {
		if (bookmarks.length === 0) {
			toast.info(t("Add markers first (M at the playhead)"));
			return;
		}
		const text = chaptersText({ bookmarks, ticksPerSecond: TICKS_PER_SECOND });
		try {
			await navigator.clipboard.writeText(text);
			toast.success(t("Chapters copied"), { description: text.split("\n").slice(0, 3).join(" · ") });
		} catch {
			toast.error(t("The clipboard is not available here"));
		}
	};

	return (
		<PanelView
			title={t("Markers")}
			actions={
				<div className="flex gap-1">
					<Button size="sm" variant="outline" onClick={copyChapters} title={t("Copy the markers as YouTube chapters")}>
						{t("Chapters")}
					</Button>
					<Button size="sm" onClick={addAtPlayhead} title={t("Add a marker at the playhead (M)")}>
						{t("Add")}
					</Button>
				</div>
			}
		>
			<div className="flex flex-col gap-1 px-3 pb-3" data-testid="markers-view">
				{bookmarks.length === 0 ? (
					<p className="text-muted-foreground text-sm">
						{t("No markers yet. Press M to add one at the playhead. I and O mark a range; Ctrl+Shift+A selects the clips in it.")}
					</p>
				) : (
					bookmarks.map((bookmark) => (
						<MarkerRow key={bookmark.time} bookmark={bookmark} fps={fps} />
					))
				)}
			</div>
		</PanelView>
	);
}

function MarkerRow({ bookmark, fps }: { bookmark: Bookmark; fps: { numerator: number; denominator: number } }) {
	const t = useT();
	const editor = useEditor();
	const [note, setNote] = useState(bookmark.note ?? "");
	useEffect(() => setNote(bookmark.note ?? ""), [bookmark.note]);
	const time = bookmark.time as MediaTime;
	const label = formatTimecode({ time, rate: fps }) ?? `${(bookmark.time / TICKS_PER_SECOND).toFixed(2)} s`;
	const lengthSeconds = bookmark.duration ? bookmark.duration / TICKS_PER_SECOND : 0;

	const commitNote = () => {
		const trimmed = note.trim();
		if (trimmed === (bookmark.note ?? "")) return;
		void editor.scenes.updateBookmark({ time, updates: { note: trimmed || undefined } });
	};

	return (
		<div className="flex items-center gap-2 rounded-md px-1 py-1 hover:bg-accent/40" data-testid="marker-row">
			<input
				type="color"
				aria-label={t("Marker colour")}
				className="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
				value={bookmark.color ?? DEFAULT_TIMELINE_BOOKMARK_COLOR}
				onChange={(event) => void editor.scenes.updateBookmark({ time, updates: { color: event.target.value } })}
			/>
			<button
				type="button"
				className="shrink-0 font-mono text-xs tabular-nums hover:underline"
				onClick={() => seekToBookmarkTime({ editor, time })}
				title={t("Go to this marker")}
			>
				{label}
				{lengthSeconds > 0 ? <span className="text-muted-foreground"> +{lengthSeconds.toFixed(1)}s</span> : null}
			</button>
			<Input
				className="h-7 flex-1 text-xs"
				placeholder={t("Note")}
				value={note}
				onChange={(event) => setNote(event.target.value)}
				onBlur={commitNote}
				onKeyDown={(event) => {
					if (event.key === "Enter") (event.target as HTMLInputElement).blur();
					event.stopPropagation();
				}}
			/>
			<Button
				size="sm"
				variant="ghost"
				className="h-7 px-2"
				onClick={() => void editor.scenes.removeBookmark({ time })}
				aria-label={t("Remove marker")}
			>
				×
			</Button>
		</div>
	);
}

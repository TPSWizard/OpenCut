/**
 * Timeline files for the open project (row 111): export to and import from
 * OpenTimelineIO, FCPXML and CMX 3600 EDL through `./timeline`.
 */

import type { EditorCore } from "@/core";
import {
	type ExchangeAsset,
	type ExchangeRate,
	ImportTimelineCommand,
	TIMELINE_FORMATS,
	type TimelineFormat,
	importTimeline,
	projectTimeline,
	readTimeline,
	writeTimeline,
} from "./timeline";

const MIME: Record<TimelineFormat, string> = { otio: "application/json", fcpxml: "application/xml", edl: "text/plain" };

export function isTimelineFormat(value: unknown): value is TimelineFormat {
	return typeof value === "string" && (TIMELINE_FORMATS as string[]).includes(value);
}

function projectBits(editor: EditorCore) {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const rate: ExchangeRate = { num: project.settings.fps.numerator, den: project.settings.fps.denominator };
	const assets: ExchangeAsset[] = editor.media.getAssets().map((asset) => ({
		id: asset.id,
		name: asset.name,
		type: asset.type,
		duration: asset.duration,
		width: asset.width,
		height: asset.height,
		hasAudio: asset.hasAudio,
	}));
	return { project, rate, assets };
}

/** A file name from a project name: no characters file systems refuse. */
function safeName(name: string): string {
	return name.replace(/[\\/:*?"<>|]+/g, " ").replace(/\s+/g, " ").trim() || "Timeline";
}

/** The open project's timeline written as `format`. */
export function exportProjectTimeline({ editor, format }: { editor: EditorCore; format: TimelineFormat }): {
	fileName: string;
	mimeType: string;
	text: string;
	notes: string[];
} {
	const { project, rate, assets } = projectBits(editor);
	const timeline = projectTimeline({
		name: project.metadata.name,
		rate,
		width: project.settings.canvasSize.width,
		height: project.settings.canvasSize.height,
		tracks: editor.scenes.getActiveScene().tracks,
		assets,
	});
	const { text, notes } = writeTimeline(timeline, format);
	return { fileName: `${safeName(project.metadata.name)}.${format}`, mimeType: MIME[format], text, notes };
}

/** Reads a timeline file onto the open project's timeline in one undo step. */
export function importProjectTimeline({ editor, text, fileName }: { editor: EditorCore; text: string; fileName: string }): {
	clips: number;
	tracks: number;
	missing: string[];
	notes: string[];
} {
	const { rate, assets } = projectBits(editor);
	const timeline = readTimeline({ text, fileName, rate });
	const result = importTimeline({ tracks: editor.scenes.getActiveScene().tracks, timeline, assets });
	if (result.clips > 0) {
		editor.command.execute({ command: new ImportTimelineCommand(result.tracks) });
	}
	return { clips: result.clips, tracks: result.trackCount, missing: result.missing, notes: timeline.notes ?? [] };
}

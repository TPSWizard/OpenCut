/**
 * Script to B-roll in the web editor: better queries from Claude through
 * the API worker (optional, needs the server passcode), then a stock search
 * per shot and the first result laid on the timeline at the shot's time,
 * all clips in one undo step. The plan itself is `./plan`.
 */

import { BatchCommand } from "@/commands";
import { InsertElementCommand } from "@/commands/timeline/element/insert-element";
import { AddTrackCommand } from "@/commands/timeline/track/add-track";
import type { EditorCore } from "@/core";
import { apiUrl } from "@/lib/api";
import { importStockItem, searchStock, type StockKind, type StockProvider } from "@/stock/stock";
import { buildElementFromMedia } from "@/timeline/element-utils";
import { mediaTimeFromSeconds, mediaTimeToSeconds } from "@/wasm";
import type { Shot } from "./plan";

/** Asks the API worker for Claude's query for each shot's line; the shots keep their timings. */
export async function improveQueries({
	shots,
	passcode,
	fetchImpl = fetch,
}: {
	shots: Shot[];
	passcode: string;
	fetchImpl?: typeof fetch;
}): Promise<{ shots: Shot[]; model: string }> {
	if (shots.length === 0) return { shots, model: "" };
	const response = await fetchImpl(apiUrl("/broll/plan"), {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": passcode },
		body: JSON.stringify({ lines: shots.map((shot) => shot.text) }),
	});
	const body = (await response.json().catch(() => null)) as { queries?: unknown; model?: unknown; error?: unknown } | null;
	if (!response.ok) {
		throw new Error(typeof body?.error === "string" ? body.error : `Claude search terms failed (${response.status})`);
	}
	const queries = body?.queries;
	if (!Array.isArray(queries) || queries.length !== shots.length || !queries.every((query) => typeof query === "string")) {
		throw new Error("The server's answer did not have one query per line");
	}
	return {
		shots: shots.map((shot, index) => ({ ...shot, query: (queries[index] as string).trim() || shot.query })),
		model: typeof body?.model === "string" ? body.model : "",
	};
}

export interface ShotResult {
	shot: Shot;
	/** The picture or video placed, by name. */
	placed?: string;
	/** Why nothing was placed. */
	problem?: string;
}

/**
 * For each shot: searches `provider` for `kind`, imports the first result
 * (with its credit) and places it at `at` + the shot's start for the shot's
 * length (a video shorter than that is used whole). The clips go on a new
 * video track above the others (the main track would pull the first clip
 * to 0 s), all in one undo step.
 */
export async function assembleBroll({
	editor,
	shots,
	provider = "openverse",
	kind = "image",
	at,
	onProgress,
}: {
	editor: EditorCore;
	shots: Shot[];
	provider?: StockProvider;
	kind?: StockKind;
	/** Timeline seconds the first shot starts at (default: the playhead). */
	at?: number;
	onProgress?: (done: number, total: number) => void;
}): Promise<ShotResult[]> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const origin = at ?? mediaTimeToSeconds({ time: editor.playback.getCurrentTime() });
	const results: ShotResult[] = [];
	const track = new AddTrackCommand({ type: "video" });
	const inserts: InsertElementCommand[] = [];
	for (const [index, shot] of shots.entries()) {
		try {
			const { items } = await searchStock({ provider, kind, query: shot.query });
			const item = items[0];
			if (!item) {
				results.push({ shot, problem: "No results" });
			} else {
				const { mediaId, name } = await importStockItem({ editor, item, addToTimeline: false });
				const media = editor.media.getAssets().find((asset) => asset.id === mediaId);
				const seconds = media?.type === "video" && media.duration ? Math.min(shot.duration, media.duration) : shot.duration;
				const element = buildElementFromMedia({
					mediaId,
					mediaType: media?.type ?? kind,
					name: shot.query,
					duration: mediaTimeFromSeconds({ seconds }),
					startTime: mediaTimeFromSeconds({ seconds: origin + shot.start }),
				});
				inserts.push(new InsertElementCommand({ element, placement: { mode: "explicit", trackId: track.getTrackId() } }));
				results.push({ shot, placed: name });
			}
		} catch (error) {
			results.push({ shot, problem: error instanceof Error ? error.message : String(error) });
		}
		onProgress?.(index + 1, shots.length);
	}
	if (inserts.length > 0) {
		editor.command.execute({ command: new BatchCommand([track, ...inserts]) });
	}
	return results;
}

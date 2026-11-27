/**
 * Smooth jump cuts (row 106): the jump cuts on the timeline (two video
 * clips back to back on one track from the same file, the file jumping
 * between them), the bridge planned across each, the bridge clip, and the
 * command that puts bridges on the track above. The shared `jumpcut` crate
 * (through WASM) decides all of it, as the desktop editor's
 * `editor::jump_cuts` does.
 */

import { jumpCutFind, jumpCutFrameChoices, jumpCutPlan } from "opencut-wasm";
import { Command, type CommandResult } from "@/commands/base-command";
import { EditorCore as Core } from "@/core";
import { bakeAnimatedParams } from "@/freeze/freeze";
import type { ParamValues } from "@/params";
import { getSourceTimeAtClipTime } from "@/retime";
import { buildEmptyTrack } from "@/timeline/placement/track-factory";
import type { SceneTracks, TimelineElement, TimelineTrack, VideoElement, VideoTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND, mediaTimeFromSeconds } from "@/wasm";

/** Set on bridge clips, so a cut is not smoothed twice. */
export const JUMP_CUT_BRIDGE_PARAM = "jumpcut.bridge";
/** The name of a track made to hold bridges. */
export const JUMP_CUT_TRACK_NAME = "Smooth cuts";

const choices = jumpCutFrameChoices();
/** The bridge lengths offered, in frames. */
export const JUMP_CUT_FRAMES: number[] = Array.from(choices.slice(0, -1));
export const JUMP_CUT_DEFAULT_FRAMES: number = choices[choices.length - 1];

export interface JumpCut {
	trackId: string;
	beforeId: string;
	afterId: string;
	/** Timeline ticks. */
	time: number;
	/** How far the file jumps, seconds (negative: back). */
	sourceGap: number;
	/** A bridge already covers it. */
	smoothed: boolean;
}

export interface JumpCutBridgeJob {
	cut: JumpCut;
	mediaId: string;
	/** Seconds on the timeline. */
	start: number;
	frames: number;
	duration: number;
	/** Each frame's share of the way from the frame before to the frame after. */
	alphas: number[];
	/** Source seconds of the frame shown just before the bridge, and just after it. */
	fromSource: number;
	toSource: number;
}

/** Top to bottom, as drawn: overlay tracks (first is the top), then the main track. */
function visualTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main];
}

function isBridge(element: TimelineElement): boolean {
	return "params" in element && element.params[JUMP_CUT_BRIDGE_PARAM] === true;
}

/** The source seconds a video clip shows `clipTime` ticks after its start. */
function sourceSeconds(element: VideoElement, clipTime: number): number {
	const offset = getSourceTimeAtClipTime({ clipTime, retime: element.retime, duration: element.duration, ticksPerSecond: TICKS_PER_SECOND });
	return (element.trimStart + offset) / TICKS_PER_SECOND;
}

function findVideo(tracks: SceneTracks, elementId: string): VideoElement | null {
	for (const track of visualTracks(tracks)) {
		const element = (track.elements as TimelineElement[]).find((candidate) => candidate.id === elementId);
		if (element) return element.type === "video" ? element : null;
	}
	return null;
}

/** The jump cuts between visible video clips on unlocked video tracks. */
export function projectJumpCuts({ tracks, fps }: { tracks: SceneTracks; fps: number }): JumpCut[] {
	const ordered = visualTracks(tracks);
	const clips: { track: TimelineTrack; element: VideoElement }[] = [];
	const media: string[] = [];
	const flat: number[] = [];
	ordered.forEach((track, trackIndex) => {
		if (track.type !== "video" || track.locked) return;
		for (const element of track.elements as TimelineElement[]) {
			if (element.type !== "video" || element.hidden || isBridge(element)) continue;
			let mediaIndex = media.indexOf(element.mediaId);
			if (mediaIndex < 0) mediaIndex = media.push(element.mediaId) - 1;
			clips.push({ track, element });
			flat.push(
				trackIndex,
				mediaIndex,
				element.startTime / TICKS_PER_SECOND,
				(element.startTime + element.duration) / TICKS_PER_SECOND,
				sourceSeconds(element, 0),
				sourceSeconds(element, element.duration),
			);
		}
	});
	const found = jumpCutFind(new Float64Array(flat), Math.max(1, fps));
	const cuts: JumpCut[] = [];
	for (let i = 0; i + 3 < found.length; i += 4) {
		const before = clips[found[i]];
		const after = clips[found[i + 1]];
		const time = after.element.startTime;
		const index = ordered.indexOf(before.track);
		const above = index > 0 ? ordered[index - 1] : undefined;
		const smoothed = !!above?.elements.some(
			(element) => isBridge(element as TimelineElement) && element.startTime <= time && time < element.startTime + element.duration,
		);
		cuts.push({ trackId: before.track.id, beforeId: before.element.id, afterId: after.element.id, time, sourceGap: found[i + 3], smoothed });
	}
	return cuts;
}

/** Plans a bridge of `frames` frames across `cut` (fewer when the clips are short; null when not even two fit). */
export function planJumpCutBridge({
	tracks,
	cut,
	fps,
	frames,
}: {
	tracks: SceneTracks;
	cut: JumpCut;
	fps: number;
	frames: number;
}): JumpCutBridgeJob | null {
	const before = findVideo(tracks, cut.beforeId);
	const after = findVideo(tracks, cut.afterId);
	if (!before || !after) return null;
	const plan = jumpCutPlan(
		cut.time / TICKS_PER_SECOND,
		before.startTime / TICKS_PER_SECOND,
		(after.startTime + after.duration) / TICKS_PER_SECOND,
		Math.max(1, fps),
		frames,
	);
	if (plan.length < 5) return null;
	const [start, count, duration, fromTime, toTime] = plan;
	const ticks = (seconds: number) => Math.round(seconds * TICKS_PER_SECOND);
	return {
		cut,
		mediaId: before.mediaId,
		start,
		frames: count,
		duration,
		alphas: Array.from(plan.slice(5)),
		fromSource: sourceSeconds(before, ticks(fromTime) - before.startTime),
		toSource: sourceSeconds(after, ticks(toTime) - after.startTime),
	};
}

/**
 * The bridge clip for `job`, showing `mediaId` (the in-between frames): the
 * first clip's params with animated values fixed at the cut (no sound), its
 * effects and masks.
 */
export function buildBridgeElement({ tracks, job, mediaId }: { tracks: SceneTracks; job: JumpCutBridgeJob; mediaId: string }): VideoElement | null {
	const before = findVideo(tracks, job.cut.beforeId);
	if (!before) return null;
	const localTime = Math.min(Math.max(job.cut.time - before.startTime, 0), before.duration);
	const params: ParamValues = { ...bakeAnimatedParams({ element: before, localTime }), [JUMP_CUT_BRIDGE_PARAM]: true };
	delete params.volume;
	delete params.muted;
	const duration = mediaTimeFromSeconds({ seconds: job.duration });
	return {
		id: generateUUID(),
		type: "video",
		name: `${before.name} (smooth cut)`,
		mediaId,
		startTime: mediaTimeFromSeconds({ seconds: job.start }),
		duration,
		trimStart: 0 as VideoElement["trimStart"],
		trimEnd: 0 as VideoElement["trimEnd"],
		sourceDuration: duration,
		hidden: false,
		isSourceAudioEnabled: false,
		params,
		...(before.effects?.length ? { effects: structuredClone(before.effects) } : {}),
		...(before.masks?.length ? { masks: structuredClone(before.masks) } : {}),
	};
}

/**
 * The tracks with each bridge on the video track directly above its cut's
 * track when that one is unlocked, shown and free there, else on a new
 * video track put directly above it. The cut clips are not changed.
 */
export function insertBridges({
	tracks,
	bridges,
}: {
	tracks: SceneTracks;
	bridges: { trackId: string; element: VideoElement }[];
}): { tracks: SceneTracks; placed: { trackId: string; elementId: string }[] } {
	let overlay = [...tracks.overlay];
	const main = tracks.main;
	const placed: { trackId: string; elementId: string }[] = [];
	for (const { trackId, element } of bridges) {
		const ordered = [...overlay, main];
		const index = ordered.findIndex((track) => track.id === trackId);
		if (index < 0) continue;
		const end = element.startTime + element.duration;
		const above = index > 0 ? ordered[index - 1] : undefined;
		const free =
			above &&
			above.type === "video" &&
			!above.locked &&
			!above.hidden &&
			(above.elements as TimelineElement[]).every((other) => end <= other.startTime || element.startTime >= other.startTime + other.duration);
		let target: VideoTrack;
		if (free) {
			target = above as VideoTrack;
		} else {
			target = buildEmptyTrack({ id: generateUUID(), type: "video", name: JUMP_CUT_TRACK_NAME });
			// Directly above: before the cut's track in the overlay list (the main track is below them all).
			overlay = [...overlay.slice(0, index), target, ...overlay.slice(index)];
		}
		const updated: VideoTrack = {
			...target,
			elements: [...target.elements, element].sort((a, b) => a.startTime - b.startTime),
		};
		overlay = overlay.map((track) => (track.id === updated.id ? updated : track));
		placed.push({ trackId: updated.id, elementId: element.id });
	}
	return { tracks: { ...tracks, overlay }, placed };
}

/** Puts the bridges on the timeline in one undo step. */
export class InsertJumpCutBridgesCommand extends Command {
	readonly skipsRipple = true;
	private savedState: SceneTracks | null = null;
	private placed: { trackId: string; elementId: string }[] = [];

	constructor(private readonly bridges: { trackId: string; element: VideoElement }[]) {
		super();
	}

	getPlaced(): { trackId: string; elementId: string }[] {
		return this.placed;
	}

	execute(): CommandResult | undefined {
		const editor = Core.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const result = insertBridges({ tracks: this.savedState, bridges: this.bridges });
		this.placed = result.placed;
		if (this.placed.length === 0) return undefined;
		editor.timeline.updateTracks(result.tracks);
		// The selection stays on the clip, so its panel shows the result.
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		Core.getInstance().timeline.updateTracks(this.savedState);
	}
}

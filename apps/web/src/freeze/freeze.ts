/**
 * Freeze frame: a video clip holds the frame at the playhead for a while.
 * The clip is split there, a still of that frame (decoded from the
 * original file at full size) goes in between as an image clip with the
 * clip's look (position, scale, effects, masks; animated values baked in
 * at that moment), and everything that starts at or after the playhead
 * on unlocked tracks moves along by the freeze length, so separated audio
 * and captions stay in step. One undo step. The desktop editor's
 * `freeze_frame_selected` does the same.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { resolveAnimationPathValueAtTime } from "@/animation/resolve";
import { Command, createElementSelectionResult, type CommandResult } from "@/commands/base-command";
import { SplitElementsCommand } from "@/commands/timeline/element/split-elements";
import type { EditorCore } from "@/core";
import { EditorCore as Core } from "@/core";
import { processMediaAssets } from "@/media/processing";
import type { ParamValues } from "@/params";
import { getSourceTimeAtClipTime } from "@/retime";
import { applyRippleAdjustments } from "@/ripple";
import type { ImageElement, SceneTracks, TimelineElement, TimelineTrack, VideoElement } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND, type MediaTime, ZERO_MEDIA_TIME, mediaTimeFromSeconds, mediaTimeToSeconds } from "@/wasm";

export const FREEZE_DEFAULT_SECONDS = 3;
export const FREEZE_MIN_SECONDS = 0.1;
export const FREEZE_MAX_SECONDS = 60;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/**
 * The video clip to freeze at `time`: a selected video clip under the
 * playhead, or else the topmost one. The playhead may sit on the clip's
 * first frame but not past its last.
 */
export function pickFreezeTarget({
	tracks,
	selected,
	time,
}: {
	tracks: SceneTracks;
	selected: { trackId: string; elementId: string }[];
	time: number;
}): { trackId: string; elementId: string } | null {
	const under: { trackId: string; elementId: string }[] = [];
	for (const track of allTracks(tracks)) {
		if (track.locked) continue;
		for (const element of track.elements as TimelineElement[]) {
			if (element.type !== "video" || element.hidden) continue;
			if (time >= element.startTime && time < element.startTime + element.duration) {
				under.push({ trackId: track.id, elementId: element.id });
			}
		}
	}
	const chosen = under.find((candidate) =>
		selected.some((pick) => pick.trackId === candidate.trackId && pick.elementId === candidate.elementId),
	);
	return chosen ?? under[0] ?? null;
}

/** The source time, in seconds, the clip shows `clipTime` ticks after its start (as the renderer maps it). */
export function freezeSourceSeconds({ element, clipTime }: { element: VideoElement; clipTime: number }): number {
	const inside = Math.min(Math.max(clipTime, 0), Math.max(element.duration - 1, 0));
	const source =
		element.trimStart +
		getSourceTimeAtClipTime({ clipTime: inside, retime: element.retime, duration: element.duration, ticksPerSecond: TICKS_PER_SECOND });
	const last = element.trimStart + element.duration + element.trimEnd - 1;
	return Math.min(Math.max(source, 0), Math.max(last, 0)) / TICKS_PER_SECOND;
}

/** The clip's params with every animated param fixed at its value `localTime` ticks into the clip. */
export function bakeAnimatedParams({ element, localTime }: { element: TimelineElement; localTime: number }): ParamValues {
	const params: ParamValues = { ...element.params };
	for (const path of Object.keys(element.animations ?? {})) {
		if (!(path in params)) continue;
		params[path] = resolveAnimationPathValueAtTime({
			animations: element.animations,
			propertyPath: path,
			localTime,
			fallbackValue: params[path],
		});
	}
	return params;
}

/** The still clip that stands in for `element` at `time`. */
export function buildFreezeStill({
	element,
	time,
	freeze,
	mediaId,
}: {
	element: VideoElement;
	time: MediaTime;
	freeze: MediaTime;
	mediaId: string;
}): ImageElement {
	return {
		id: generateUUID(),
		type: "image",
		name: `${element.name} (freeze)`,
		mediaId,
		startTime: time,
		duration: freeze,
		trimStart: ZERO_MEDIA_TIME,
		trimEnd: ZERO_MEDIA_TIME,
		hidden: false,
		params: bakeAnimatedParams({ element, localTime: time - element.startTime }),
		...(element.effects?.length ? { effects: structuredClone(element.effects) } : {}),
		...(element.masks?.length ? { masks: structuredClone(element.masks) } : {}),
	};
}

/**
 * The tracks after the split ones (the clip already split at `time`): the
 * still goes in at `time` on `trackId`, everything else that starts at or
 * after `time` on an unlocked track moves `freeze` later.
 */
export function insertFreezeStill({
	tracks,
	trackId,
	still,
}: {
	tracks: SceneTracks;
	trackId: string;
	still: ImageElement;
}): SceneTracks {
	const shifted = applyRippleAdjustments({
		tracks,
		adjustments: allTracks(tracks).map((track) => ({ trackId: track.id, afterTime: still.startTime, shiftAmount: -still.duration })),
	});
	const place = <T extends TimelineTrack>(track: T): T => {
		if (track.id !== trackId) return track;
		const elements = [...(track.elements as TimelineElement[]), still].sort((a, b) => a.startTime - b.startTime);
		return { ...track, elements } as T;
	};
	return { overlay: shifted.overlay.map(place), main: place(shifted.main), audio: shifted.audio.map(place) };
}

export class FreezeFrameCommand extends Command {
	readonly skipsRipple = true;
	private savedState: SceneTracks | null = null;
	private stillId: string | null = null;
	private readonly trackId: string;
	private readonly elementId: string;
	private readonly time: MediaTime;
	private readonly freeze: MediaTime;
	private readonly mediaId: string;

	constructor({ trackId, elementId, time, freeze, mediaId }: { trackId: string; elementId: string; time: MediaTime; freeze: MediaTime; mediaId: string }) {
		super();
		this.trackId = trackId;
		this.elementId = elementId;
		this.time = time;
		this.freeze = freeze;
		this.mediaId = mediaId;
	}

	/** The still clip's id once executed. */
	getStillId(): string | null {
		return this.stillId;
	}

	execute(): CommandResult | undefined {
		const editor = Core.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const track = allTracks(this.savedState).find((candidate) => candidate.id === this.trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === this.elementId);
		if (!track || track.locked || element?.type !== "video") return undefined;
		if (this.time < element.startTime || this.time >= element.startTime + element.duration) return undefined;
		const still = buildFreezeStill({ element, time: this.time, freeze: this.freeze, mediaId: this.mediaId });
		if (this.time > element.startTime) {
			new SplitElementsCommand({ elements: [{ trackId: this.trackId, elementId: this.elementId }], splitTime: this.time }).execute();
		}
		const split = editor.scenes.getActiveScene().tracks;
		editor.timeline.updateTracks(insertFreezeStill({ tracks: split, trackId: this.trackId, still }));
		this.stillId = still.id;
		return createElementSelectionResult([{ trackId: this.trackId, elementId: still.id }]);
	}

	undo(): void {
		if (!this.savedState) return;
		Core.getInstance().timeline.updateTracks(this.savedState);
	}
}

/** One frame of a video file, full size, as a PNG. */
async function decodeFrame({ file, seconds }: { file: File; seconds: number }): Promise<Blob> {
	const input = new Input({ source: new BlobSource(file), formats: ALL_FORMATS });
	try {
		const track = await input.getPrimaryVideoTrack();
		if (!track || !(await track.canDecode())) throw new Error("The clip's video cannot be decoded in this browser");
		const sink = new CanvasSink(track, { poolSize: 1 });
		const first = await track.getFirstTimestamp();
		const wrapped = (await sink.getCanvas(Math.max(seconds, first))) ?? (await sink.getCanvas(first));
		if (!wrapped) throw new Error("The frame at the playhead could not be decoded");
		const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
		if ("convertToBlob" in canvas) return await canvas.convertToBlob({ type: "image/png" });
		const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
		if (!blob) throw new Error("The frame could not be saved");
		return blob;
	} finally {
		input.dispose();
	}
}

export interface FreezeResult {
	assetId: string;
	trackId: string;
	/** The still clip. */
	elementId: string;
	/** Source time of the frozen frame. */
	sourceSeconds: number;
	seconds: number;
}

/**
 * Freezes the frame at `time` seconds (default: the playhead) of a video clip
 * (default: the selected one under the playhead, else the topmost) for
 * `seconds` (default 3).
 */
export async function freezeFrame({
	editor,
	trackId,
	elementId,
	time,
	seconds = FREEZE_DEFAULT_SECONDS,
}: {
	editor: EditorCore;
	trackId?: string;
	elementId?: string;
	time?: number;
	seconds?: number;
}): Promise<FreezeResult> {
	if (!Number.isFinite(seconds) || seconds < FREEZE_MIN_SECONDS || seconds > FREEZE_MAX_SECONDS) {
		throw new Error(`The freeze lasts ${FREEZE_MIN_SECONDS} to ${FREEZE_MAX_SECONDS} seconds`);
	}
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const at = time === undefined ? editor.playback.getCurrentTime() : mediaTimeFromSeconds({ seconds: time });
	const tracks = editor.scenes.getActiveScene().tracks;
	const target =
		trackId && elementId
			? { trackId, elementId }
			: pickFreezeTarget({ tracks, selected: editor.selection.getSelectedElements(), time: at });
	if (!target) throw new Error("Put the playhead over a video clip");
	const track = allTracks(tracks).find((candidate) => candidate.id === target.trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === target.elementId);
	if (!track || element?.type !== "video") throw new Error("Freeze frame needs a video clip");
	if (track.locked) throw new Error("The clip's track is locked");
	if (at < element.startTime || at >= element.startTime + element.duration) throw new Error("Put the playhead over the clip");
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");

	const sourceSeconds = freezeSourceSeconds({ element, clipTime: at - element.startTime });
	const png = await decodeFrame({ file: asset.file, seconds: sourceSeconds });
	const base = asset.name.replace(/\.[^.]+$/, "");
	const stamp = sourceSeconds.toFixed(2).replace(".", "-");
	const [processed] = await processMediaAssets({ files: [new File([png], `${base} (freeze ${stamp} s).png`, { type: "image/png" })] });
	if (!processed) throw new Error("The still could not be prepared");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool: "freeze", from: asset.id } },
	});
	if (!saved) throw new Error("The still could not be stored");

	const command = new FreezeFrameCommand({
		trackId: target.trackId,
		elementId: target.elementId,
		time: at,
		freeze: mediaTimeFromSeconds({ seconds }),
		mediaId: saved.id,
	});
	editor.command.execute({ command });
	const stillId = command.getStillId();
	if (!stillId) throw new Error("The freeze could not be placed");
	return { assetId: saved.id, trackId: target.trackId, elementId: stillId, sourceSeconds, seconds: mediaTimeToSeconds({ time: mediaTimeFromSeconds({ seconds }) }) };
}

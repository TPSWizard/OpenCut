/**
 * Project interchange (row 111): the project's timeline as the shared
 * `interchange` crate's neutral timeline (through WASM) for export to
 * OpenTimelineIO, FCPXML or a CMX 3600 EDL, and an imported timeline back
 * onto tracks in one undo step, as the desktop editor's
 * `editor::project_timeline` / `editor::timeline_import` do.
 *
 * Export takes video, picture and audio clips with their place, trims,
 * constant speed, hidden state, whether a video clip's sound plays, and
 * transitions (wipes as wipes, every other kind as a dissolve); the notes
 * say what else was left out. Import puts each track on a new track (video
 * above the existing ones, bottom first; audio at the bottom), or onto the
 * main track when the timeline is empty. The web has no file paths: a
 * file is matched to imported media by its name; clips of files not
 * imported yet are skipped and the files named.
 */

import { interchangeFormatNotes, interchangeRead, interchangeWrite } from "opencut-wasm";
import { Command, type CommandResult } from "@/commands/base-command";
import { EditorCore as Core } from "@/core";
import { getSourceTimeAtClipTime } from "@/retime";
import { buildEmptyTrack } from "@/timeline/placement/track-factory";
import { buildElementFromMedia } from "@/timeline/element-utils";
import type { AudioTrack, SceneTracks, TimelineElement, TimelineTrack, VideoElement, VideoTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND, mediaTimeFromSeconds } from "@/wasm";

export type TimelineFormat = "otio" | "fcpxml" | "edl";
export const TIMELINE_FORMATS: TimelineFormat[] = ["otio", "fcpxml", "edl"];

export const NOTE_TEXT = "Text, captions, stickers and graphics were left out.";
export const NOTE_COMPOUND = "Compound clips were left out.";
export const NOTE_LOOK = "Effects, masks, keyframes and changes of position, size and opacity were left out.";
export const NOTE_SPEED = "Speed ramps and reversed clips went out as a constant forward speed.";
export const NOTE_TRANSITION = "Transitions other than dissolves and wipes went out as dissolves.";
export const NOTE_FADE_IN = "Transitions in from nothing were left out.";
export const NOTE_LIBRARY = "Library sounds were left out.";

export interface ExchangeRate {
	num: number;
	den: number;
}

export interface ExchangeMedia {
	id: string;
	name: string;
	url: string;
	duration?: number;
	hasVideo: boolean;
	hasAudio: boolean;
	still: boolean;
	width?: number;
	height?: number;
}

export interface ExchangeClip {
	name: string;
	mediaId: string;
	start: number;
	duration: number;
	sourceStart: number;
	speed: number;
	sound: boolean;
	enabled: boolean;
	transition?: { frames: number; kind: string };
}

export interface ExchangeTrack {
	kind: "video" | "audio";
	name: string;
	clips: ExchangeClip[];
}

export interface ExchangeTimeline {
	name: string;
	rate: ExchangeRate;
	width: number;
	height: number;
	media: ExchangeMedia[];
	tracks: ExchangeTrack[];
	notes?: string[];
}

/** What the timeline needs to know about a media asset. */
export interface ExchangeAsset {
	id: string;
	name: string;
	type: string;
	duration?: number;
	width?: number;
	height?: number;
	hasAudio?: boolean;
}

const fpsOf = (rate: ExchangeRate) => rate.num / Math.max(1, rate.den);
// Rust's f64::round (half away from zero), as the crate rounds.
const roundHalfAway = (value: number) => (value < 0 ? -Math.round(-value) : Math.round(value));
const toFrames = (rate: ExchangeRate, ticks: number) => roundHalfAway((ticks / TICKS_PER_SECOND) * fpsOf(rate));
const toTicks = (rate: ExchangeRate, frames: number) => mediaTimeFromSeconds({ seconds: Math.max(0, frames / fpsOf(rate)) });

function changesLook(element: TimelineElement): boolean {
	const p = (element.params ?? {}) as Record<string, unknown>;
	const num = (key: string, fallback: number) => (typeof p[key] === "number" ? (p[key] as number) : fallback);
	const anyOf = (value: unknown) => Array.isArray(value) && value.length > 0;
	const animations = element.animations as Record<string, unknown> | undefined;
	return (
		anyOf((element as VideoElement).effects) ||
		anyOf((element as VideoElement).masks) ||
		(!!animations && Object.keys(animations).length > 0) ||
		!!(element as VideoElement).cutout ||
		Math.abs(num("transform.positionX", 0)) > 1e-6 ||
		Math.abs(num("transform.positionY", 0)) > 1e-6 ||
		Math.abs(num("transform.scaleX", 1) - 1) > 1e-6 ||
		Math.abs(num("transform.scaleY", 1) - 1) > 1e-6 ||
		Math.abs(num("transform.rotate", 0)) > 1e-6 ||
		Math.abs(num("opacity", 1) - 1) > 1e-6
	);
}

/** The project's timeline for export (`notes` say what was left out). */
export function projectTimeline({
	name,
	rate,
	width,
	height,
	tracks,
	assets,
}: {
	name: string;
	rate: ExchangeRate;
	width: number;
	height: number;
	tracks: SceneTracks;
	assets: ExchangeAsset[];
}): ExchangeTimeline {
	const timeline: ExchangeTimeline = { name, rate, width, height, media: [], tracks: [], notes: [] };
	const note = (text: string) => {
		if (!timeline.notes?.includes(text)) timeline.notes?.push(text);
	};
	const used = new Set<string>();
	// Video tracks from the bottom: the main track, then the overlays upwards.
	const video: TimelineTrack[] = [tracks.main, ...[...tracks.overlay].reverse()];
	const lists: [ExchangeTrack["kind"], TimelineTrack[]][] = [
		["video", video],
		["audio", tracks.audio],
	];
	for (const [kind, list] of lists) {
		for (const track of list) {
			if (track.type === "text" || track.type === "graphic") {
				if (track.elements.length > 0) note(NOTE_TEXT);
				continue;
			}
			if (track.type === "effect") continue;
			const out: ExchangeTrack = { kind, name: track.name, clips: [] };
			const elements = [...(track.elements as TimelineElement[])].sort((a, b) => a.startTime - b.startTime);
			elements.forEach((element, i) => {
				if (element.type === "compound") {
					note(NOTE_COMPOUND);
					return;
				}
				if (element.type !== "video" && element.type !== "image" && element.type !== "audio") {
					note(NOTE_TEXT);
					return;
				}
				if (element.type === "audio" && element.sourceType !== "upload") {
					note(NOTE_LIBRARY);
					return;
				}
				const mediaId = element.mediaId;
				const asset = assets.find((candidate) => candidate.id === mediaId);
				if (!asset) return;
				used.add(mediaId);
				if (changesLook(element)) note(NOTE_LOOK);
				let speed = 1;
				const retime = "retime" in element ? element.retime : undefined;
				if (retime && (retime.reverse || (retime.ramp?.length ?? 0) > 0 || Math.abs(retime.rate - 1) > 1e-9)) {
					if (retime.reverse || (retime.ramp?.length ?? 0) > 0) note(NOTE_SPEED);
					const span = getSourceTimeAtClipTime({ clipTime: element.duration, retime, duration: element.duration, ticksPerSecond: TICKS_PER_SECOND });
					speed = element.duration > 0 ? Math.round((Math.abs(span) / element.duration) * 1e6) / 1e6 : 1;
				}
				if ("transitionIn" in element && element.transitionIn) note(NOTE_FADE_IN);
				const next = elements[i + 1];
				const nextStartsHere = !!next && next.startTime === element.startTime + element.duration;
				const t = "transition" in element ? element.transition : undefined;
				let transition: ExchangeClip["transition"];
				if (t && nextStartsHere) {
					const wipe = t.type.includes("Wipe");
					if (!wipe && t.type !== "Fade") note(NOTE_TRANSITION);
					transition = { frames: Math.max(1, toFrames(timeline.rate, t.duration)), kind: wipe ? "wipe" : "dissolve" };
				}
				const sound =
					element.type === "audio"
						? true
						: element.type === "video"
							? !!asset.hasAudio && element.isSourceAudioEnabled !== false && element.params?.muted !== true
							: false;
				out.clips.push({
					name: element.name,
					mediaId,
					start: toFrames(timeline.rate, element.startTime),
					duration: toFrames(timeline.rate, element.duration),
					sourceStart: toFrames(timeline.rate, element.trimStart),
					speed,
					sound,
					enabled: !("hidden" in element && element.hidden),
					...(transition ? { transition } : {}),
				});
			});
			if (out.clips.length > 0 || (kind === "video" && timeline.tracks.length === 0)) timeline.tracks.push(out);
		}
	}
	for (const asset of assets) {
		if (!used.has(asset.id)) continue;
		timeline.media.push({
			id: asset.id,
			name: asset.name,
			url: asset.name,
			...(asset.duration != null && asset.type !== "image" ? { duration: roundHalfAway(asset.duration * fpsOf(rate)) } : {}),
			hasVideo: asset.type === "video" || asset.type === "image",
			hasAudio: !!asset.hasAudio || asset.type === "audio",
			still: asset.type === "image",
			...(asset.width ? { width: asset.width } : {}),
			...(asset.height ? { height: asset.height } : {}),
		});
	}
	if (timeline.notes?.length === 0) delete timeline.notes;
	return timeline;
}

/** The timeline written as `format`, and every note (the timeline's and the format's). */
export function writeTimeline(timeline: ExchangeTimeline, format: TimelineFormat): { text: string; notes: string[] } {
	const json = JSON.stringify(timeline);
	const formatNotes = JSON.parse(interchangeFormatNotes(json, format)) as string[];
	return { text: interchangeWrite(json, format), notes: [...(timeline.notes ?? []), ...formatNotes] };
}

/** Reads a timeline file (an EDL at `rate`). */
export function readTimeline({ text, fileName, rate }: { text: string; fileName: string; rate: ExchangeRate }): ExchangeTimeline {
	return JSON.parse(interchangeRead(text, fileName, rate.num, rate.den)) as ExchangeTimeline;
}

/** The asset among `assets` with the timeline file's name (letter case ignored). */
export function findAsset<T extends ExchangeAsset>(assets: T[], media: ExchangeMedia): T | undefined {
	const name = media.name.toLowerCase();
	return assets.find((asset) => asset.name.toLowerCase() === name);
}

export interface TimelineImport {
	tracks: SceneTracks;
	clips: number;
	trackCount: number;
	missing: string[];
}

/** `tracks` with `timeline` brought in (see the module comment). */
export function importTimeline({
	tracks,
	timeline,
	assets,
}: {
	tracks: SceneTracks;
	timeline: ExchangeTimeline;
	assets: ExchangeAsset[];
}): TimelineImport {
	const rate = timeline.rate;
	const missing: string[] = [];
	const empty = [tracks.main, ...tracks.overlay, ...tracks.audio].every((track) => track.elements.length === 0);
	let main: VideoTrack = tracks.main;
	let overlay = [...tracks.overlay];
	let audio = [...tracks.audio];
	let clips = 0;
	let trackCount = 0;
	timeline.tracks.forEach((track, index) => {
		const isAudio = track.kind === "audio";
		const elements: TimelineElement[] = [];
		for (const clip of track.clips) {
			const media = timeline.media.find((candidate) => candidate.id === clip.mediaId);
			if (!media) continue;
			const asset = findAsset(assets, media);
			if (!asset) {
				if (!missing.includes(media.name)) missing.push(media.name);
				continue;
			}
			const element = clipElement({ clip, asset, rate, audio: isAudio });
			if (element) elements.push(element);
		}
		if (elements.length === 0) return;
		clips += elements.length;
		trackCount += 1;
		const sorted = elements.sort((a, b) => a.startTime - b.startTime);
		// A transition needs its next clip, which may not have come in.
		sorted.forEach((element, i) => {
			const next = sorted[i + 1];
			if ("transition" in element && element.transition && next?.startTime !== element.startTime + element.duration) {
				delete (element as { transition?: unknown }).transition;
			}
		});
		if (!isAudio && index === 0 && empty) {
			main = { ...main, elements: sorted as VideoTrack["elements"] };
		} else if (isAudio) {
			const created = buildEmptyTrack({ id: generateUUID(), type: "audio", name: track.name }) as AudioTrack;
			audio = [...audio, { ...created, elements: sorted as AudioTrack["elements"] }];
		} else {
			const created = buildEmptyTrack({ id: generateUUID(), type: "video", name: track.name }) as VideoTrack;
			// A new video track goes on top, so the timeline's stacking is kept.
			overlay = [{ ...created, elements: sorted as VideoTrack["elements"] }, ...overlay];
		}
	});
	return { tracks: { ...tracks, main, overlay, audio }, clips, trackCount, missing };
}

function clipElement({
	clip,
	asset,
	rate,
	audio,
}: {
	clip: ExchangeClip;
	asset: ExchangeAsset;
	rate: ExchangeRate;
	audio: boolean;
}): TimelineElement | null {
	const duration = toTicks(rate, clip.duration);
	if (duration < TICKS_PER_SECOND / 100) return null;
	const startTime = toTicks(rate, clip.start);
	const type = audio ? "audio" : asset.type === "audio" ? "audio" : asset.type === "image" || asset.type === "lottie" ? "image" : "video";
	const element = {
		...buildElementFromMedia({
			mediaId: asset.id,
			mediaType: type as Parameters<typeof buildElementFromMedia>[0]["mediaType"],
			name: clip.name.trim() || asset.name,
			duration,
			startTime,
		}),
		id: generateUUID(),
	} as TimelineElement;
	const speed = Math.abs(clip.speed);
	if (element.type !== "image") {
		const trimStart = toTicks(rate, Math.max(0, clip.sourceStart));
		const source = asset.duration != null ? mediaTimeFromSeconds({ seconds: asset.duration }) : undefined;
		const span = Math.round(duration * speed);
		Object.assign(element, {
			trimStart,
			...(source != null ? { sourceDuration: source, trimEnd: Math.max(0, source - trimStart - span) } : {}),
			...(Math.abs(speed - 1) > 1e-6 && speed > 0 ? { retime: { rate: speed, ...(clip.speed < 0 ? { reverse: true } : {}) } } : {}),
		});
	}
	if (element.type === "video" || element.type === "image") {
		element.hidden = !clip.enabled;
		if (clip.transition) {
			element.transition = { type: clip.transition.kind === "wipe" ? "DirectionalWipe" : "Fade", duration: toTicks(rate, clip.transition.frames) };
		}
	}
	if (element.type === "video" && asset.hasAudio && !clip.sound) {
		element.isSourceAudioEnabled = false;
	}
	return element;
}

/** Puts an imported timeline on the timeline in one undo step. */
export class ImportTimelineCommand extends Command {
	readonly skipsRipple = true;
	private savedState: SceneTracks | null = null;

	constructor(private readonly tracks: SceneTracks) {
		super();
	}

	execute(): CommandResult | undefined {
		const editor = Core.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		editor.timeline.updateTracks(this.tracks);
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		Core.getInstance().timeline.updateTracks(this.savedState);
	}
}

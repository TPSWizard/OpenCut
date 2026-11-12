/**
 * Compound clips and groups. A compound clip lives on a video track and
 * holds other clips in its own time (0 is the start of its content); it is
 * moved, trimmed and split like a media clip, `sourceDuration` being the
 * content's length. Renderers and the audio mix never see it: they call
 * `expandCompounds`, which swaps each compound for its children clipped to
 * the compound's visible window, on tracks of their own placed where the
 * compound sits. The desktop's `crates/editor/src/compound.rs` follows the
 * same rules, and both read and write the same JSON.
 *
 * Grouped clips share a `groupId`: clicking one selects them all.
 */

import { splitAnimationsAtTime, clampAnimationsToDuration } from "@/animation/keyframes";
import { getSourceTimeAtClipTime } from "@/retime";
import { buildEmptyTrack } from "@/timeline/placement/track-factory";
import type {
	CompoundElement,
	ElementRef,
	SceneTracks,
	TimelineElement,
	TimelineTrack,
	VideoTrack,
} from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND } from "@/wasm";

const MAX_NESTING = 8;

export function isCompoundElement(element: { type: string }): element is CompoundElement {
	return element.type === "compound";
}

function allTracksOf(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

export function hasCompounds(tracks: SceneTracks): boolean {
	return allTracksOf(tracks).some((track) => (track.elements as TimelineElement[]).some(isCompoundElement));
}

export function compoundChildCount(element: TimelineElement): number {
	return isCompoundElement(element) ? element.tracks.reduce((sum, track) => sum + track.elements.length, 0) : 0;
}

/**
 * `element` with `cutStart` taken off its start and `cutEnd` off its end
 * (ticks of its own time): the same source frames stay at the same
 * timeline times; keyframes, caption words and ramp keys follow. The start
 * time is left as is.
 */
export function clipElement<T extends TimelineElement>({
	element,
	cutStart,
	cutEnd,
}: {
	element: T;
	cutStart: number;
	cutEnd: number;
}): T {
	const start = Math.max(0, cutStart);
	const end = Math.max(0, cutEnd);
	if (start === 0 && end === 0) return element;
	const duration = element.duration;
	const nextDuration = Math.max(0, duration - start - end);
	const retime = "retime" in element ? element.retime : undefined;
	let advance = start;
	if (retime) {
		if (retime.reverse) {
			// Backwards: the window's start is what the clip shows last.
			const span = (time: number) => getSourceTimeAtClipTime({ clipTime: time, retime: { ...retime, reverse: false }, ticksPerSecond: TICKS_PER_SECOND });
			advance = span(duration) - span(duration - end);
		} else {
			advance = getSourceTimeAtClipTime({ clipTime: start, retime, ticksPerSecond: TICKS_PER_SECOND });
		}
	}
	const clipped = { ...element } as T & Record<string, unknown>;
	clipped.trimStart = Math.round(element.trimStart + advance) as T["trimStart"];
	clipped.duration = Math.round(nextDuration) as T["duration"];
	if (element.sourceDuration !== undefined) {
		const shown = retime
			? getSourceTimeAtClipTime({ clipTime: nextDuration, retime: { ...retime, reverse: false }, ticksPerSecond: TICKS_PER_SECOND })
			: nextDuration;
		clipped.trimEnd = Math.max(0, Math.round(element.sourceDuration - clipped.trimStart - shown)) as T["trimEnd"];
	}
	let animations = element.animations;
	if (start > 0 && animations) {
		animations = splitAnimationsAtTime({ animations, splitTime: start as never }).rightAnimations;
	}
	if (animations) {
		animations = clampAnimationsToDuration({ animations, duration: nextDuration as never });
	}
	clipped.animations = animations;
	if (start > 0 && "words" in element && element.words) {
		clipped.words = element.words
			.map((word) => ({ ...word, start: word.start - start, end: word.end - start }))
			.filter((word) => word.end > 0 && word.start < nextDuration);
	}
	if (start > 0 && retime?.ramp) {
		const shift = start / TICKS_PER_SECOND;
		clipped.retime = { ...retime, ramp: retime.ramp.map((key) => ({ ...key, time: key.time - shift })) };
	}
	return clipped as T;
}

/**
 * A compound's children as ordinary clips on timeline time, clipped to its
 * visible window, one list per inner track (top to bottom). Ids are
 * `<compound id>~<child id>` so they never collide with real clips.
 */
export function compoundChildren(compound: CompoundElement): { track: TimelineTrack; elements: TimelineElement[] }[] {
	const windowStart = compound.trimStart;
	const windowEnd = compound.trimStart + compound.duration;
	return compound.tracks.map((track) => ({
		track,
		elements: (track.elements as TimelineElement[]).flatMap((child) => {
			const start = child.startTime;
			const end = child.startTime + child.duration;
			if (end <= windowStart || start >= windowEnd) return [];
			const clipped = clipElement({
				element: child,
				cutStart: Math.max(0, windowStart - start),
				cutEnd: Math.max(0, end - windowEnd),
			});
			if (clipped.duration <= 0) return [];
			const placed = {
				...clipped,
				id: `${compound.id}~${child.id}`,
				startTime: compound.startTime + (Math.max(start, windowStart) - windowStart),
			} as TimelineElement;
			if (compound.hidden && "hidden" in placed) (placed as { hidden?: boolean }).hidden = true;
			return [placed];
		}),
	}));
}

function expandTrackList({
	tracks,
	depth,
}: {
	tracks: TimelineTrack[];
	depth: number;
}): { visual: TimelineTrack[][]; audio: TimelineTrack[]; hosts: TimelineTrack[] } {
	// For each host track: the synthetic visual tracks that go above it.
	const visual: TimelineTrack[][] = [];
	const audio: TimelineTrack[] = [];
	const hosts: TimelineTrack[] = [];
	for (const track of tracks) {
		const compounds = (track.elements as TimelineElement[]).filter(isCompoundElement);
		if (compounds.length === 0) {
			visual.push([]);
			hosts.push(track);
			continue;
		}
		const above: TimelineTrack[] = [];
		for (const compound of compounds) {
			for (const { track: inner, elements } of compoundChildren(compound)) {
				const synthetic = {
					...inner,
					id: `${track.id}~${compound.id}~${inner.id}`,
					elements: [...elements].sort((a, b) => a.startTime - b.startTime),
					...("muted" in track && track.muted ? { muted: true } : {}),
					...("hidden" in track && track.hidden ? { hidden: true } : {}),
				} as TimelineTrack;
				if (synthetic.type === "audio") audio.push(synthetic);
				else above.push(synthetic);
			}
		}
		let expandedAbove = above;
		if (depth + 1 < MAX_NESTING && above.some((candidate) => (candidate.elements as TimelineElement[]).some(isCompoundElement))) {
			const nested = expandTrackList({ tracks: above, depth: depth + 1 });
			expandedAbove = nested.hosts.flatMap((host, index) => [...nested.visual[index], host]);
			audio.push(...nested.audio);
		}
		visual.push(expandedAbove);
		hosts.push({
			...track,
			elements: (track.elements as TimelineElement[]).filter((element) => !isCompoundElement(element)),
		} as TimelineTrack);
	}
	return { visual, audio, hosts };
}

/** The tracks as renderers and the mixer see them: compounds swapped for their clipped children. */
export function expandCompounds(tracks: SceneTracks): SceneTracks {
	if (!hasCompounds(tracks)) return tracks;
	const overlayResult = expandTrackList({ tracks: tracks.overlay, depth: 0 });
	const mainResult = expandTrackList({ tracks: [tracks.main], depth: 0 });
	const audioResult = expandTrackList({ tracks: tracks.audio, depth: 0 });
	const overlay = [
		...overlayResult.hosts.flatMap((host, index) => [...overlayResult.visual[index], host]),
		// What sits in a compound on the main track draws just above it.
		...mainResult.visual[0],
	];
	return {
		overlay: overlay as SceneTracks["overlay"],
		main: mainResult.hosts[0] as VideoTrack,
		audio: [...audioResult.hosts, ...overlayResult.audio, ...mainResult.audio, ...audioResult.audio] as SceneTracks["audio"],
	};
}

// ---------------------------------------------------------------- groups

/** The clips grouped with `ref` (itself included); just itself when it is not grouped. */
export function groupRefs({ tracks, ref }: { tracks: SceneTracks; ref: ElementRef }): ElementRef[] {
	const all = allTracksOf(tracks);
	const element = all.find((track) => track.id === ref.trackId)?.elements.find((candidate) => candidate.id === ref.elementId) as
		| TimelineElement
		| undefined;
	const groupId = element?.groupId;
	if (!groupId) return [ref];
	return all.flatMap((track) =>
		(track.elements as TimelineElement[])
			.filter((candidate) => candidate.groupId === groupId && !track.locked)
			.map((candidate) => ({ trackId: track.id, elementId: candidate.id })),
	);
}

function mapElements(tracks: SceneTracks, update: (element: TimelineElement) => TimelineElement): SceneTracks {
	const mapTrack = <T extends TimelineTrack>(track: T): T => ({ ...track, elements: (track.elements as TimelineElement[]).map(update) }) as T;
	return { overlay: tracks.overlay.map(mapTrack), main: mapTrack(tracks.main), audio: tracks.audio.map(mapTrack) } as SceneTracks;
}

export function groupElements({ tracks, refs, groupId }: { tracks: SceneTracks; refs: ElementRef[]; groupId: string }): SceneTracks {
	const ids = new Set(refs.map((ref) => ref.elementId));
	if (ids.size < 2) throw new Error("Select two or more clips to group");
	return mapElements(tracks, (element) => (ids.has(element.id) ? { ...element, groupId } : element));
}

export function ungroupElements({ tracks, refs }: { tracks: SceneTracks; refs: ElementRef[] }): SceneTracks {
	const ids = new Set(refs.map((ref) => ref.elementId));
	const groups = new Set(
		allTracksOf(tracks)
			.flatMap((track) => track.elements as TimelineElement[])
			.filter((element) => ids.has(element.id) && element.groupId)
			.map((element) => element.groupId as string),
	);
	if (groups.size === 0) throw new Error("Nothing to ungroup");
	return mapElements(tracks, (element) => {
		if (!element.groupId || !groups.has(element.groupId)) return element;
		const { groupId: _groupId, ...rest } = element;
		return rest as TimelineElement;
	});
}

// ---------------------------------------------------------------- create / break apart

function isFree(track: TimelineTrack, start: number, end: number): boolean {
	return !track.locked && !(track.elements as TimelineElement[]).some((element) => start < element.startTime + element.duration && end > element.startTime);
}

/**
 * Takes the clips off the timeline into one compound clip spanning them, on
 * the topmost video track they came from when it is free (else a new video
 * track at the top). Returns the new tracks and the compound's place.
 */
export function createCompound({
	tracks,
	refs,
	compoundId = generateUUID(),
}: {
	tracks: SceneTracks;
	refs: ElementRef[];
	compoundId?: string;
}): { tracks: SceneTracks; ref: ElementRef } {
	const all = allTracksOf(tracks);
	const picked: { track: TimelineTrack; element: TimelineElement }[] = [];
	for (const ref of refs) {
		const track = all.find((candidate) => candidate.id === ref.trackId);
		const element = track?.elements.find((candidate) => candidate.id === ref.elementId) as TimelineElement | undefined;
		if (!track || !element) continue;
		if (track.locked) throw new Error(`Track "${track.name}" is locked`);
		if (!picked.some((entry) => entry.element.id === element.id)) picked.push({ track, element });
	}
	if (picked.length === 0) throw new Error("Select the clips to combine");
	const start = Math.min(...picked.map(({ element }) => element.startTime));
	const end = Math.max(...picked.map(({ element }) => element.startTime + element.duration));
	const ids = new Set(picked.map(({ element }) => element.id));
	const order = all.map((track) => track.id);
	const sourceTracks = [...new Set(picked.map(({ track }) => track.id))].sort((a, b) => order.indexOf(a) - order.indexOf(b));
	const inner: TimelineTrack[] = sourceTracks.map((trackId) => {
		const source = all.find((track) => track.id === trackId) as TimelineTrack;
		const elements = picked
			.filter(({ track }) => track.id === trackId)
			.map(({ element }) => ({ ...element, startTime: element.startTime - start }) as TimelineElement)
			.sort((a, b) => a.startTime - b.startTime);
		const { locked: _locked, ...rest } = source;
		return { ...rest, id: generateUUID(), elements } as unknown as TimelineTrack;
	});
	const without = mapTracks(
		tracks,
		(track) =>
			({
				...track,
				elements: (track.elements as TimelineElement[]).filter((element) => !ids.has(element.id)),
			}) as TimelineTrack,
	);
	const compound: CompoundElement = {
		id: compoundId,
		type: "compound",
		name: "Compound clip",
		startTime: start as CompoundElement["startTime"],
		duration: (end - start) as CompoundElement["duration"],
		trimStart: 0 as CompoundElement["trimStart"],
		trimEnd: 0 as CompoundElement["trimEnd"],
		sourceDuration: (end - start) as CompoundElement["sourceDuration"],
		params: {},
		tracks: inner,
	};
	const withoutAll = allTracksOf(without);
	const host = sourceTracks
		.map((trackId) => withoutAll.find((track) => track.id === trackId) as TimelineTrack)
		.find((track) => track.type === "video" && isFree(track, start, end));
	if (host) {
		return {
			tracks: mapTracks(without, (track) =>
				track.id === host.id ? ({ ...track, elements: [...track.elements, compound].sort((a, b) => a.startTime - b.startTime) } as TimelineTrack) : track,
			),
			ref: { trackId: host.id, elementId: compoundId },
		};
	}
	const track = { ...buildEmptyTrack({ id: generateUUID(), type: "video" }), elements: [compound] } as VideoTrack;
	return { tracks: { ...without, overlay: [track, ...without.overlay] }, ref: { trackId: track.id, elementId: compoundId } };
}

function mapTracks(tracks: SceneTracks, update: (track: TimelineTrack) => TimelineTrack): SceneTracks {
	return {
		overlay: tracks.overlay.map(update),
		main: update(tracks.main) as VideoTrack,
		audio: tracks.audio.map(update),
	} as SceneTracks;
}

/** Puts a compound's visible content back on the timeline as clips (grouped) and removes it. */
export function breakApartCompound({ tracks, ref }: { tracks: SceneTracks; ref: ElementRef }): { tracks: SceneTracks; refs: ElementRef[] } {
	const all = allTracksOf(tracks);
	const host = all.find((track) => track.id === ref.trackId);
	const compound = host?.elements.find((element) => element.id === ref.elementId) as TimelineElement | undefined;
	if (!host || !compound || !isCompoundElement(compound)) throw new Error("Select one compound clip");
	if (host.locked) throw new Error(`Track "${host.name}" is locked`);
	let next = mapTracks(tracks, (track) =>
		track.id === host.id ? ({ ...track, elements: (track.elements as TimelineElement[]).filter((element) => element.id !== compound.id) } as TimelineTrack) : track,
	);
	const existingIds = new Set(allTracksOf(next).flatMap((track) => track.elements.map((element) => element.id)));
	const pieces = compoundChildren(compound).flatMap(({ track, elements }) => elements.map((element) => ({ type: track.type, element })));
	const groupId = pieces.length > 1 ? generateUUID() : undefined;
	const refs: ElementRef[] = [];
	for (const { type, element } of pieces) {
		const original = element.id.startsWith(`${compound.id}~`) ? element.id.slice(compound.id.length + 1) : element.id;
		const id = existingIds.has(original) ? generateUUID() : original;
		existingIds.add(id);
		const piece = { ...element, id, ...(groupId ? { groupId } : {}) } as TimelineElement;
		const start = piece.startTime;
		const end = piece.startTime + piece.duration;
		const candidates = type === "audio" ? next.audio : type === "video" ? [next.main, ...next.overlay] : next.overlay;
		const target = candidates.find((track) => track.type === type && isFree(track, start, end));
		if (target) {
			next = mapTracks(next, (track) =>
				track.id === target.id ? ({ ...track, elements: [...track.elements, piece].sort((a, b) => a.startTime - b.startTime) } as TimelineTrack) : track,
			);
			refs.push({ trackId: target.id, elementId: id });
		} else {
			const created = { ...buildEmptyTrack({ id: generateUUID(), type }), elements: [piece] } as TimelineTrack;
			next = (type === "audio" ? { ...next, audio: [...next.audio, created] } : { ...next, overlay: [created, ...next.overlay] }) as SceneTracks;
			refs.push({ trackId: created.id, elementId: id });
		}
	}
	return { tracks: next, refs };
}

/**
 * Multicam groups in the web editor: camera angles of one scene, each a
 * video clip on its own track, lined up by their sound, with a cut list
 * saying which angle shows when (the shared Rust `multicam` crate through
 * WASM). The group lives in the clips' params - `multicam.group`,
 * `multicam.angle` (0 for the angle the others were lined up to) and
 * `multicam.cuts` (the same on every angle) - and only the angle a cut
 * picks is opaque (stepped opacity keys); only the first angle's sound
 * plays. The desktop builds the same commands (`editor::multicam_group`).
 */

import { multicamAddCut, multicamCutCount, multicamOpacityKeys } from "opencut-wasm";
import type { ScalarChannel } from "@/animation/types";
import { BatchCommand } from "@/commands";
import type { Command } from "@/commands/base-command";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import { MoveElementCommand } from "@/commands/timeline/element/move-elements";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { SceneTracks, TimelineElement, TimelineTrack, VideoElement } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { type MediaTime, roundMediaTime, TICKS_PER_SECOND } from "@/wasm";

export const MULTICAM_GROUP = "multicam.group";
export const MULTICAM_ANGLE = "multicam.angle";
export const MULTICAM_CUTS = "multicam.cuts";

export interface MulticamAngle {
	trackId: string;
	element: VideoElement;
	angle: number;
	/** Timeline seconds. */
	start: number;
	end: number;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

const seconds = (ticks: number) => ticks / TICKS_PER_SECOND;
const ticks = (value: number) => roundMediaTime({ time: Math.max(0, value) * TICKS_PER_SECOND }) as MediaTime;

function findElement(tracks: SceneTracks, elementId: string): { track: TimelineTrack; element: TimelineElement } | null {
	for (const track of allTracks(tracks)) {
		const element = (track.elements as TimelineElement[]).find((candidate) => candidate.id === elementId);
		if (element) return { track, element };
	}
	return null;
}

/** The group `elementId` belongs to, by angle, or null when it is not in one. */
export function multicamGroup(tracks: SceneTracks, elementId: string): { group: string; angles: MulticamAngle[] } | null {
	const found = findElement(tracks, elementId);
	const group = found && "params" in found.element ? found.element.params[MULTICAM_GROUP] : undefined;
	if (typeof group !== "string" || group.length === 0) return null;
	const angles: MulticamAngle[] = [];
	for (const track of allTracks(tracks)) {
		for (const element of track.elements as TimelineElement[]) {
			if (element.type !== "video" || element.params[MULTICAM_GROUP] !== group) continue;
			const angle = element.params[MULTICAM_ANGLE];
			angles.push({
				trackId: track.id,
				element,
				angle: typeof angle === "number" && angle >= 0 ? Math.floor(angle) : 0,
				start: seconds(element.startTime),
				end: seconds(element.startTime + element.duration),
			});
		}
	}
	angles.sort((a, b) => a.angle - b.angle);
	return { group, angles };
}

/** The group's stored cut list (from any angle that has one). */
export function groupCuts(angles: MulticamAngle[]): string {
	for (const angle of angles) {
		const cuts = angle.element.params[MULTICAM_CUTS];
		if (typeof cuts === "string" && cuts.length > 0) return cuts;
	}
	return "";
}

export function cutCount(angles: MulticamAngle[]): number {
	return multicamCutCount(groupCuts(angles));
}

/**
 * Lines angles up: each gets its reference start plus its lag (everything
 * later if one would start before zero), joins a new group as angle 0, 1,
 * 2…, and all but the first are muted. Every angle must be a video clip on
 * its own track, free where it lands.
 */
export function multicamSyncCommand({
	tracks,
	lags,
	groupId = generateUUID(),
}: {
	tracks: SceneTracks;
	lags: { elementId: string; lag: number }[];
	groupId?: string;
}): Command {
	if (lags.length < 2) throw new Error("Select two or more video clips to sync");
	const placed: { track: TimelineTrack; element: VideoElement; lag: number }[] = [];
	const used = new Set<string>();
	for (const { elementId, lag } of lags) {
		const found = findElement(tracks, elementId);
		if (!found) throw new Error("A clip to sync is no longer on the timeline");
		if (found.element.type !== "video") throw new Error("Multicam angles must be video clips");
		if (used.has(found.track.id)) throw new Error("Put each angle on its own track");
		used.add(found.track.id);
		placed.push({ track: found.track, element: found.element, lag });
	}
	const referenceStart = seconds(placed[0].element.startTime);
	const lowest = Math.min(...placed.map(({ lag }) => referenceStart + lag));
	const shift = Math.max(0, -lowest);
	const commands: Command[] = [];
	placed.forEach(({ track, element, lag }, index) => {
		const start = ticks(referenceStart + lag + shift);
		if (start !== element.startTime) {
			const end = start + element.duration;
			const blocked = (track.elements as TimelineElement[]).some(
				(other) => other.id !== element.id && other.startTime < end && start < other.startTime + other.duration,
			);
			if (blocked) throw new Error("Lining an angle up would overlap another clip on its track");
			commands.push(
				new MoveElementCommand({
					moves: [{ sourceTrackId: track.id, targetTrackId: track.id, elementId: element.id, newStartTime: start }],
				}),
			);
		}
		commands.push(
			new UpdateElementsCommand({
				updates: [
					{
						trackId: track.id,
						elementId: element.id,
						patch: {
							params: { ...element.params, [MULTICAM_GROUP]: groupId, [MULTICAM_ANGLE]: index, [MULTICAM_CUTS]: "", muted: index > 0 },
						} as Partial<TimelineElement>,
					},
				],
			}),
		);
	});
	return new BatchCommand(commands);
}

function applyCuts(angles: MulticamAngle[], cuts: string): Command {
	const commands: Command[] = [];
	for (const angle of angles) {
		const flat = multicamOpacityKeys(cuts, angle.angle, angle.start, angle.end);
		const keys: ScalarChannel["keys"] = [];
		for (let index = 0; index + 1 < flat.length; index += 2) {
			keys.push({ id: generateUUID(), time: ticks(flat[index]), value: flat[index + 1], segmentToNext: "linear", tangentMode: "auto" });
		}
		commands.push(
			new SetChannelCommand({
				trackId: angle.trackId,
				elementId: angle.element.id,
				propertyPath: "opacity",
				channel: keys.length > 0 ? ({ keys } as ScalarChannel) : undefined,
			}),
		);
		commands.push(
			new UpdateElementsCommand({
				updates: [
					{
						trackId: angle.trackId,
						elementId: angle.element.id,
						patch: { params: { ...angle.element.params, [MULTICAM_CUTS]: cuts } } as Partial<TimelineElement>,
					},
				],
			}),
		);
	}
	return new BatchCommand(commands);
}

/**
 * Switches the group of `elementId` to `angle` (0-based) from timeline
 * second `time` on; `angle` null takes every cut out.
 */
export function multicamSwitchCommand({
	tracks,
	elementId,
	time,
	angle,
}: {
	tracks: SceneTracks;
	elementId: string;
	time: number;
	angle: number | null;
}): Command {
	const group = multicamGroup(tracks, elementId);
	if (!group) throw new Error("The clip is not in a multicam group");
	if (angle === null) return applyCuts(group.angles, "");
	if (!group.angles.some((candidate) => candidate.angle === angle)) throw new Error(`The group has no angle ${angle + 1}`);
	return applyCuts(group.angles, multicamAddCut(groupCuts(group.angles), time, angle));
}

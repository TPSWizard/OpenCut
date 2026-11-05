import { Command, type CommandResult, createElementSelectionResult } from "@/commands/base-command";
import { EditorCore } from "@/core";
import { rippleShiftElements } from "@/ripple";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline";
import type { RippleScope } from "@/silence/detect";
import { type MediaTime, roundMediaTime } from "@/wasm";
import { SplitElementsCommand } from "./split-elements";

export interface TimeRange {
	start: MediaTime;
	end: MediaTime;
}

/**
 * Cuts stretches out of one element in a single undo step: the element is
 * split at each range's edges, the pieces inside the ranges are removed,
 * and (with a ripple scope) everything that starts after a removed stretch
 * moves left to close the gap, on the element's track or on every track.
 * Elements on other tracks that straddle a removed stretch are left where
 * they are.
 */
export class RemoveRangesCommand extends Command {
	private savedState: SceneTracks | null = null;
	private remaining: { trackId: string; elementId: string }[] = [];
	private readonly trackId: string;
	private readonly elementId: string;
	private readonly ranges: TimeRange[];
	private readonly ripple: RippleScope;

	constructor({
		trackId,
		elementId,
		ranges,
		ripple,
	}: {
		trackId: string;
		elementId: string;
		ranges: TimeRange[];
		ripple: RippleScope;
	}) {
		super();
		this.trackId = trackId;
		this.elementId = elementId;
		this.ranges = [...ranges].sort((a, b) => a.start - b.start);
		this.ripple = ripple;
	}

	/** The pieces of the element left on the timeline after the cut. */
	getRemainingElements(): { trackId: string; elementId: string }[] {
		return this.remaining;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const original = this.findElement({ elementId: this.elementId });
		if (!original) {
			this.remaining = [];
			return undefined;
		}
		// Pieces descend from the original; they are found by position.
		let pieceIds = new Set<string>([this.elementId]);

		// Latest first, so earlier cuts keep their timeline positions.
		for (const range of [...this.ranges].reverse()) {
			for (const edge of [range.end, range.start]) {
				const host = this.pieceContaining({ pieceIds, time: edge });
				if (!host) continue;
				const split = new SplitElementsCommand({
					elements: [{ trackId: this.trackId, elementId: host.id }],
					splitTime: edge,
				});
				split.execute();
				for (const created of split.getRightSideElements()) {
					pieceIds.add(created.elementId);
				}
			}
			const track = this.track();
			const inside = track.elements.filter(
				(element) =>
					pieceIds.has(element.id) &&
					element.startTime >= range.start - 1 &&
					element.startTime + element.duration <= range.end + 1,
			);
			if (inside.length > 0) {
				const removed = new Set(inside.map((element) => element.id));
				pieceIds = new Set([...pieceIds].filter((id) => !removed.has(id)));
				editor.timeline.updateTracks(
					mapTracks({
						tracks: editor.scenes.getActiveScene().tracks,
						update: (candidate) =>
							candidate.id === this.trackId
								? ({
										...candidate,
										elements: candidate.elements.filter(
											(element) => !removed.has(element.id),
										),
									} as TimelineTrack)
								: candidate,
					}),
				);
			}
			if (this.ripple !== "none") {
				const shift = roundMediaTime({ time: range.end - range.start });
				editor.timeline.updateTracks(
					mapTracks({
						tracks: editor.scenes.getActiveScene().tracks,
						update: (candidate) =>
							this.ripple === "all" || candidate.id === this.trackId
								? ({
										...candidate,
										elements: rippleShiftElements({
											elements: candidate.elements as TimelineElement[],
											afterTime: range.end,
											shiftAmount: shift,
										}),
									} as TimelineTrack)
								: candidate,
					}),
				);
			}
		}

		this.remaining = this.track()
			.elements.filter((element) => pieceIds.has(element.id))
			.map((element) => ({ trackId: this.trackId, elementId: element.id }));
		return createElementSelectionResult(this.remaining);
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}

	private track(): TimelineTrack {
		const tracks = EditorCore.getInstance().scenes.getActiveScene().tracks;
		const track = [...tracks.overlay, tracks.main, ...tracks.audio].find(
			(candidate) => candidate.id === this.trackId,
		);
		if (!track) {
			throw new Error("The element's track is gone");
		}
		return track;
	}

	private findElement({ elementId }: { elementId: string }) {
		return this.track().elements.find((element) => element.id === elementId) ?? null;
	}

	/** The piece that `time` falls strictly inside, if any. */
	private pieceContaining({ pieceIds, time }: { pieceIds: Set<string>; time: MediaTime }) {
		return (
			this.track().elements.find(
				(element) =>
					pieceIds.has(element.id) &&
					time > element.startTime &&
					time < element.startTime + element.duration,
			) ?? null
		);
	}
}

/** Applies `update` to every track; it must hand back the same kind of track. */
function mapTracks({
	tracks,
	update,
}: {
	tracks: SceneTracks;
	update: (track: TimelineTrack) => TimelineTrack;
}): SceneTracks {
	return {
		overlay: tracks.overlay.map((track) => update(track) as typeof track),
		main: update(tracks.main) as typeof tracks.main,
		audio: tracks.audio.map((track) => update(track) as typeof track),
	};
}

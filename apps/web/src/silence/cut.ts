import type { EditorCore } from "@/core";
import { RemoveRangesCommand } from "@/commands/timeline/element/remove-ranges";
import { listAudioElements, mixedLevels } from "@/ducking/duck";
import { collectAudioElements, createAudioContext } from "@/media/audio";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import {
	SILENCE_DEFAULTS,
	coveredSeconds,
	silenceIntervals,
	type Interval,
	type RippleScope,
	type SilenceOptions,
} from "./detect";

/** Sample rate the clip is measured at; silence needs no more. */
const ANALYSIS_SAMPLE_RATE = 16_000;

export interface SilenceReport {
	/** Stretches that would be removed, in timeline seconds. */
	intervals: Interval[];
	removedSeconds: number;
	clipStart: number;
	clipDuration: number;
}

/** Finds the silent stretches of a clip without changing anything. */
export async function detectSilence({
	editor,
	trackId,
	elementId,
	options = {},
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<SilenceOptions>;
}): Promise<SilenceReport> {
	const settings: SilenceOptions = { ...SILENCE_DEFAULTS, ...options };
	const tracks = editor.scenes.getActiveScene().tracks;
	const target = listAudioElements({ tracks }).find(
		(candidate) => candidate.trackId === trackId && candidate.element.id === elementId,
	);
	if (!target) {
		throw new Error("The element is not an audio or video clip");
	}
	const audioContext = createAudioContext({ sampleRate: ANALYSIS_SAMPLE_RATE });
	let collected: Awaited<ReturnType<typeof collectAudioElements>>;
	try {
		collected = await collectAudioElements({
			tracks,
			mediaAssets: editor.media.getAssets(),
			audioContext,
		});
	} finally {
		void audioContext.close();
	}
	const clip = collected.find((candidate) => candidate.timelineElement.id === elementId);
	if (!clip) {
		throw new Error("The clip's audio could not be decoded, or its track is muted");
	}
	const clipStart = clip.startTime;
	const clipDuration = clip.duration;
	const levels = mixedLevels({
		voices: [clip],
		from: clipStart,
		to: clipStart + clipDuration,
		windowSeconds: settings.windowSeconds,
	});
	const intervals = silenceIntervals({
		levels,
		clipStart,
		clipEnd: clipStart + clipDuration,
		options: settings,
	});
	return {
		intervals,
		removedSeconds: coveredSeconds({ intervals }),
		clipStart,
		clipDuration,
	};
}

export interface SilenceCutResult extends SilenceReport {
	/** The pieces of the clip left on the timeline. */
	pieces: { trackId: string; elementId: string }[];
}

/** Finds the silent stretches of a clip and cuts them out in one undo step. */
export async function cutSilence({
	editor,
	trackId,
	elementId,
	ripple = "all",
	options = {},
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	ripple?: RippleScope;
	options?: Partial<SilenceOptions>;
}): Promise<SilenceCutResult> {
	const report = await detectSilence({ editor, trackId, elementId, options });
	if (report.intervals.length === 0) {
		return { ...report, pieces: [{ trackId, elementId }] };
	}
	const command = new RemoveRangesCommand({
		trackId,
		elementId,
		ranges: report.intervals.map((interval) => ({
			start: roundMediaTime({ time: interval.start * TICKS_PER_SECOND }),
			end: roundMediaTime({ time: interval.end * TICKS_PER_SECOND }),
		})),
		ripple,
	});
	// The command closes its own gaps; the editor's ripple mode would
	// otherwise shift the track a second time.
	const rippleMode = editor.command.isRippleEnabled;
	editor.command.isRippleEnabled = false;
	try {
		editor.command.execute({ command });
	} finally {
		editor.command.isRippleEnabled = rippleMode;
	}
	return { ...report, pieces: command.getRemainingElements() };
}

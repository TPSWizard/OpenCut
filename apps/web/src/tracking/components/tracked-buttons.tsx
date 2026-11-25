"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TrackRef } from "../follow";
import { MASK_TRACK_PARAM, setMaskTrack, trackCornerPin, trackMask } from "../tracked";

function useRun() {
	const t = useT();
	const [status, setStatus] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const run = async (work: (onProgress: (fraction: number) => void) => Promise<string>) => {
		setBusy(true);
		setStatus(t("Tracking…"));
		try {
			setStatus(await work((fraction) => setStatus(t("Tracking… {percent}%", { percent: Math.round(fraction * 100) }))));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(false);
		}
	};
	return { status, busy, run };
}

/** Tracked corner pin (row 92): the pinned clip's four corners follow a surface in the video. */
export function TrackCornerPinButton({ video, pinned }: { video: TrackRef; pinned: TrackRef }) {
	const t = useT();
	const editor = useEditor();
	const { status, busy, run } = useRun();
	return (
		<div className="flex flex-col gap-1">
			<p className="text-muted-foreground text-xs">
				{t("Or pin a picture or clip's corners over a screen or sign at the playhead; its four corners are then tracked and the pin follows them.")}
			</p>
			<Button
				size="sm"
				variant="outline"
				disabled={busy}
				data-testid="track-corner-pin"
				onClick={() =>
					run(async (onProgress) => {
						const result = await trackCornerPin({ editor, video, pinned, onProgress });
						return t("Corner pin follows: {keys} keys from {frames} frames; lost in {lost}.", result as unknown as Record<string, number>);
					})
				}
			>
				{t("Track corner pin")}
			</Button>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

/** Tracked mask (row 92): the video's mask follows its centre through the video. */
export function TrackMaskButtons({ video, mask }: { video: TrackRef; mask: { params: unknown } }) {
	const t = useT();
	const editor = useEditor();
	const { status, busy, run } = useRun();
	const tracked = typeof (mask.params as Record<string, unknown>)[MASK_TRACK_PARAM] === "string" && (mask.params as Record<string, string>)[MASK_TRACK_PARAM] !== "";
	return (
		<div className="flex flex-col gap-1 px-4 pb-3" data-testid="track-mask">
			<div className="flex items-center gap-2">
				<Button
					size="sm"
					variant="outline"
					disabled={busy}
					data-testid="track-mask-button"
					onClick={() =>
						run(async (onProgress) => {
							const result = await trackMask({ editor, video, onProgress });
							return t("The mask follows: {frames} frames; lost in {lost}.", { frames: result.frames, lost: result.lost });
						})
					}
				>
					{tracked ? t("Track again") : t("Track mask")}
				</Button>
				{tracked && (
					<Button size="sm" variant="text" disabled={busy} onClick={() => setMaskTrack({ editor, video, track: "" })}>
						{t("Stop following")}
					</Button>
				)}
			</div>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

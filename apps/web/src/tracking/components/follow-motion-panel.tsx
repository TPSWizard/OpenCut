"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TimelineElement } from "@/timeline/types";
import { followMotion, type TrackRef } from "../follow";
import { TrackCornerPinButton } from "./tracked-buttons";

/**
 * Motion tracking for two selected clips: a video and a sticker, text,
 * picture or clip placed over something in it at the playhead. The video is
 * the first video selected; the other follows.
 */
export function FollowMotionPanel({ targets }: { targets: readonly TrackRef[] }) {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const [status, setStatus] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	if (targets.length !== 2) return null;
	const all = [...tracks.overlay, tracks.main, ...tracks.audio];
	const typeOf = (ref: TrackRef) =>
		(all.find((track) => track.id === ref.trackId)?.elements as TimelineElement[] | undefined)?.find((element) => element.id === ref.elementId)?.type;
	const videoIndex = targets.findIndex((ref) => typeOf(ref) === "video");
	if (videoIndex < 0) return null;
	const video = targets[videoIndex];
	const follower = targets[1 - videoIndex];

	return (
		<div className="flex w-full max-w-64 flex-col gap-2 px-4" data-testid="follow-motion-panel">
			<p className="text-xs font-medium">{t("Follow motion")}</p>
			<p className="text-muted-foreground text-xs">
				{t("Place the sticker, text or clip over what it should follow at the playhead; it then follows that spot in the video until either clip ends.")}
			</p>
			<Button
				size="sm"
				variant="outline"
				disabled={busy}
				data-testid="follow-motion"
				onClick={async () => {
					setBusy(true);
					setStatus(t("Tracking…"));
					try {
						const result = await followMotion({
							editor,
							video,
							follower,
							onProgress: (fraction) => setStatus(t("Tracking… {percent}%", { percent: Math.round(fraction * 100) })),
						});
						setStatus(
							result.lost > 0
								? t("Following: {keys} keys from {frames} frames; lost in {lost} of them.", { keys: result.keys, frames: result.frames, lost: result.lost })
								: t("Following: {keys} keys from {frames} frames.", { keys: result.keys, frames: result.frames }),
						);
					} catch (error) {
						setStatus(error instanceof Error ? error.message : String(error));
					} finally {
						setBusy(false);
					}
				}}
			>
				{t("Track and follow")}
			</Button>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
			{["video", "image", "sticker"].includes(typeOf(follower) ?? "") && <TrackCornerPinButton video={video} pinned={follower} />}
		</div>
	);
}

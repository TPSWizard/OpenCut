"use client";

import { useState } from "react";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { frameRateToFloat } from "@/fps/utils";
import { useT } from "@/i18n";
import type { VideoElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { JUMP_CUT_DEFAULT_FRAMES, JUMP_CUT_FRAMES, projectJumpCuts } from "../cuts";
import { smoothJumpCuts } from "../make";

/**
 * Smooth jump cuts (row 106) for a video clip: how many jump cuts the
 * timeline has and how many touch this clip, a bridge length, and buttons
 * that bridge this clip's cuts or all of them.
 */
export function JumpCutSection({ element }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const fps = useEditor((e) => {
		const project = e.project.getActive();
		return project ? frameRateToFloat(project.settings.fps) : 30;
	});
	const [frames, setFrames] = useState(JUMP_CUT_DEFAULT_FRAMES);
	const [busy, setBusy] = useState<number | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const cuts = projectJumpCuts({ tracks, fps });
	const here = cuts.filter((cut) => cut.beforeId === element.id || cut.afterId === element.id);
	const openHere = here.filter((cut) => !cut.smoothed).length;
	const openAll = cuts.filter((cut) => !cut.smoothed).length;
	const smoothed = cuts.length - openAll;
	if (cuts.length === 0) return null;

	const run = async (only: boolean) => {
		setBusy(0);
		setStatus(null);
		try {
			const result = await smoothJumpCuts({
				editor,
				elementId: only ? element.id : undefined,
				frames,
				onProgress: (fraction) => setBusy(fraction),
			});
			setStatus(
				result.tooShort > 0
					? t("Jump cuts smoothed: {count}. The in-between frames are on the track above; too short to bridge: {short}.", {
							count: result.smoothed,
							short: result.tooShort,
						})
					: t("Jump cuts smoothed: {count}. The in-between frames are on the track above.", { count: result.smoothed }),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Smoothing the jump cuts failed"));
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey={`${element.id}:jump-cuts`} collapsible defaultOpen={openHere > 0}>
			<SectionHeader>
				<SectionTitle>{t("Smooth jump cuts")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="jump-cut-section">
					<p className="text-muted-foreground text-xs">
						{t(
							"Where words or a pause were cut out of one take, in-between frames made from the motion bridge the jump; the clips and their sound stay as they are. Jump cuts on the timeline: {all}; at this clip: {here}; smoothed: {smoothed}.",
							{ all: cuts.length, here: here.length, smoothed },
						)}
					</p>
					<div className="flex items-center gap-1">
						<span className="text-muted-foreground mr-1 text-xs">{t("Frames")}</span>
						{JUMP_CUT_FRAMES.map((count) => (
							<Button
								key={count}
								size="sm"
								variant="outline"
								disabled={busy !== null}
								className={cn("flex-1", frames === count && "border-primary text-primary")}
								onClick={() => setFrames(count)}
								data-testid={`jump-cut-frames-${count}`}
							>
								{count}
							</Button>
						))}
					</div>
					<div className="flex gap-1">
						<Button
							size="sm"
							variant="outline"
							className="flex-1"
							disabled={busy !== null || openHere === 0}
							onClick={() => void run(true)}
							data-testid="jump-cut-smooth-here"
						>
							{t("Smooth this clip's cuts")}
						</Button>
						<Button
							size="sm"
							variant="outline"
							className="flex-1"
							disabled={busy !== null || openAll === 0}
							onClick={() => void run(false)}
							data-testid="jump-cut-smooth-all"
						>
							{t("Smooth every jump cut")}
						</Button>
					</div>
					{busy !== null && (
						<p className="text-muted-foreground text-xs" role="status">
							{t("Making in-between frames from the motion…")} {Math.round(busy * 100)}%
						</p>
					)}
					{busy === null && status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="jump-cut-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

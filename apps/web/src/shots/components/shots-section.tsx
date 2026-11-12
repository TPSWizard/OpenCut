"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Section,
	SectionContent,
	SectionHeader,
	SectionTitle,
} from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import type { VideoElement } from "@/timeline/types";
import { SHOT_DEFAULTS } from "../detect";
import { applyShotCuts, scanShots, type ShotScan } from "../scan";

/**
 * Shot detection for a video clip: scans it for scene changes and turns
 * the cuts into bookmarks or splits.
 */
export function ShotsSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const editor = useEditor();
	const [threshold, setThreshold] = useState(SHOT_DEFAULTS.threshold);
	const [scan, setScan] = useState<ShotScan | null>(null);
	const [busy, setBusy] = useState(false);
	const [progress, setProgress] = useState(0);
	const [status, setStatus] = useState<string | null>(null);

	const run = async () => {
		setBusy(true);
		setStatus(null);
		setProgress(0);
		try {
			const result = await scanShots({
				editor,
				trackId,
				elementId: element.id,
				options: { threshold },
				onProgress: setProgress,
			});
			setScan(result);
			setStatus(
				result.cuts.length === 0
					? `No cuts found in ${result.framesScored} frames; try a lower threshold.`
					: `${result.cuts.length} cut${result.cuts.length === 1 ? "" : "s"} in ${result.seconds.toFixed(1)} s.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Shot detection failed");
		} finally {
			setBusy(false);
		}
	};

	const place = (apply: "bookmarks" | "split") => {
		if (!scan) return;
		const count = applyShotCuts({ editor, trackId, elementId: element.id, cuts: scan.cuts, apply });
		setStatus(apply === "split" ? `Split into ${count + 1} clips.` : `${count} bookmark${count === 1 ? "" : "s"} added.`);
		if (apply === "split") setScan(null);
	};

	return (
		<Section sectionKey={`${element.id}:shots`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Shot detection</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="shots-section">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="shots-threshold">Threshold (0–100)</Label>
						<Input
							id="shots-threshold"
							type="number"
							min={1}
							max={100}
							step={1}
							value={threshold}
							onChange={(event) => {
								const next = Number(event.target.value);
								if (Number.isFinite(next)) setThreshold(Math.min(100, Math.max(1, next)));
							}}
						/>
					</div>
					<p className="text-muted-foreground text-xs">
						Compares {SHOT_DEFAULTS.sampleFps} frames a second by colour; a change above the
						threshold is a cut, cuts closer than {SHOT_DEFAULTS.minGapSeconds} s merge.
					</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={run} disabled={busy}>
							{busy ? `Scanning ${Math.round(progress * 100)}%` : "Scan"}
						</Button>
						<Button size="sm" variant="outline" disabled={busy || !scan || scan.cuts.length === 0} onClick={() => place("bookmarks")}>
							Bookmark cuts
						</Button>
						<Button size="sm" variant="outline" disabled={busy || !scan || scan.cuts.length === 0} onClick={() => place("split")}>
							Split at cuts
						</Button>
					</div>
					{scan && scan.cuts.length > 0 && (
						<p className="text-muted-foreground text-xs" data-testid="shots-cuts">
							{scan.cuts.map((cut) => `${cut.seconds.toFixed(2)} s (${Math.round(cut.score)})`).join(", ")}
						</p>
					)}
					{status && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

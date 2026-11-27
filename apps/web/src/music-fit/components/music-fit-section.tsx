"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TimelineElement } from "@/timeline";
import { TICKS_PER_SECOND } from "@/wasm";
import { canFitMusic, fitMusic, musicFitDefaultLength } from "../fit";

/**
 * Fit to length (row 101) on an audio clip: cuts bars out or repeats bars
 * where the music sounds the same either side, so it ends with the video
 * or at a typed length.
 */
export function MusicFitSection({ element, trackId }: { element: TimelineElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const [typed, setTyped] = useState("");
	if (!canFitMusic(element)) return null;
	const videoLength = musicFitDefaultLength(editor, element.id);

	const run = async (seconds?: number) => {
		setBusy(true);
		setStatus(t("Finding the beats and bars that sound alike…"));
		try {
			const { fit, length } = await fitMusic({ editor, elementId: element.id, trackId, seconds });
			const joins = String(fit.jumps.length);
			const what =
				fit.kind === "cut"
					? t("bars cut out at {joins} join(s)").replace("{joins}", joins)
					: fit.kind === "loop"
						? t("bars repeated at {joins} join(s)").replace("{joins}", joins)
						: t("no bars to cut, so it ends early");
			let message = `${t("Music fitted to")} ${(length / TICKS_PER_SECOND).toFixed(2)} s: ${what}`;
			if (fit.trimmed > 0.01) message += `; ${fit.trimmed.toFixed(2)} s ${t("off the end, fading out")}`;
			setStatus(`${message}.`);
		} catch (error) {
			setStatus(t(error instanceof Error ? error.message : String(error)));
		} finally {
			setBusy(false);
		}
	};

	const typedSeconds = Number.parseFloat(typed);
	return (
		<Section sectionKey={`${element.id}:music-fit`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>{t("Fit to length")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-2" data-testid="music-fit-section">
					<p className="text-muted-foreground text-xs">
						{t(
							"Shortens the music by cutting bars out, or lengthens it by repeating bars, where the music sounds the same either side, with short crossfades. Fit to video ends it with everything else; or type a length in seconds.",
						)}
					</p>
					<Button
						size="sm"
						onClick={() => run()}
						disabled={busy || videoLength === null}
						data-testid="music-fit-video"
					>
						{t("Fit to video")}
						{videoLength !== null ? ` (${(videoLength / TICKS_PER_SECOND).toFixed(2)} s)` : ""}
					</Button>
					<div className="flex items-center gap-2">
						<Input
							className="h-8"
							type="number"
							min={1}
							step={0.1}
							placeholder={t("Length (s)")}
							value={typed}
							onChange={(event) => setTyped(event.target.value)}
							data-testid="music-fit-length"
						/>
						<Button
							size="sm"
							variant="outline"
							onClick={() => run(typedSeconds)}
							disabled={busy || !(typedSeconds > 0)}
							data-testid="music-fit-run"
						>
							{t("Fit")}
						</Button>
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" data-testid="music-fit-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

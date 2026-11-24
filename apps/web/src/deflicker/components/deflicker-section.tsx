"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VideoElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { DEFLICKER_STRENGTHS, type DeflickerStrength, readDeflickerLevels, readDeflickerStrength } from "../levels";
import { setDeflicker } from "../measure";

/**
 * Deflicker for a video clip: pick a strength; the clip's frames are
 * measured once (the first time), then each is evened out against its
 * neighbours as it is drawn.
 */
export function DeflickerSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState<number | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const current = readDeflickerStrength(element.params);
	const measured = readDeflickerLevels(element.params, element.mediaId)?.frames() ?? null;
	const label = (id: DeflickerStrength) =>
		id === "off" ? t("Off") : id === "light" ? t("Light") : id === "medium" ? t("Medium") : t("Strong");

	const pick = async (strength: DeflickerStrength) => {
		setBusy(0);
		setStatus(null);
		try {
			const result = await setDeflicker({
				editor,
				trackId,
				elementId: element.id,
				strength,
				onProgress: (fraction) => setBusy(fraction),
			});
			if (result.measured) setStatus(t("Deflicker on: {frames} frames measured.", { frames: result.frames }));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Deflicker failed"));
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey={`${element.id}:deflicker`} collapsible defaultOpen={current !== "off"}>
			<SectionHeader>
				<SectionTitle>{t("Deflicker")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="deflicker-section">
					<p className="text-muted-foreground text-xs">
						{measured === null
							? t("Evens out flicker from timelapses and LED or fluorescent light. The clip's frames are measured once when you pick a strength.")
							: t("Evens out flicker from timelapses and LED or fluorescent light. {frames} frames measured.", { frames: measured })}
					</p>
					<div className="flex gap-1">
						{DEFLICKER_STRENGTHS.map((id) => (
							<Button
								key={id}
								size="sm"
								variant="outline"
								disabled={busy !== null}
								className={cn("flex-1", current === id && "border-primary text-primary")}
								onClick={() => void pick(id)}
								data-testid={`deflicker-${id}`}
							>
								{label(id)}
							</Button>
						))}
					</div>
					{busy !== null && (
						<p className="text-muted-foreground text-xs" role="status">
							{t("Measuring the clip's brightness, frame by frame…")} {Math.round(busy * 100)}%
						</p>
					)}
					{busy === null && status && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

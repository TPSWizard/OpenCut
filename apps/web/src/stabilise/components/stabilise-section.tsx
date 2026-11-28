"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VideoElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { setStabilise } from "../measure";
import {
	STABILISE_STRENGTHS,
	type StabiliseStrength,
	readStabilisePath,
	readStabiliseStrength,
	stabiliseZoom,
} from "../steady";

/**
 * Stabilise for a video clip (row 107): pick a strength; the camera's
 * movement is measured once (the first time), then each frame is moved as
 * it is drawn so the camera holds steady. The file is not rewritten.
 */
export function StabiliseSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState<number | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const current = readStabiliseStrength(element.params);
	const measured = readStabilisePath(element.params, element.mediaId)?.frames() ?? null;
	const zoom = stabiliseZoom(element.params, element.mediaId);
	const label = (id: StabiliseStrength) =>
		id === "off"
			? t("Off")
			: id === "light"
				? t("Light")
				: id === "medium"
					? t("Medium")
					: id === "strong"
						? t("Strong")
						: t("Still");

	const pick = async (strength: StabiliseStrength) => {
		setBusy(0);
		setStatus(null);
		try {
			const result = await setStabilise({
				editor,
				trackId,
				elementId: element.id,
				strength,
				onProgress: (fraction) => setBusy(fraction),
			});
			if (result.measured) {
				setStatus(
					t("Stabilise on: {frames} frames measured; zoomed in {zoom}× to hide the edges.", {
						frames: result.frames,
						zoom: result.zoom.toFixed(2),
					}),
				);
			}
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Stabilising failed"));
		} finally {
			setBusy(null);
		}
	};

	const note =
		measured === null
			? t(
					"Holds a shaky camera steady without rewriting the file. The camera's movement is measured once when you pick a strength; Still holds it as on a tripod.",
				)
			: zoom && zoom.needed > zoom.zoom + 1e-6
				? t(
						"Holds a shaky camera steady without rewriting the file. {frames} frames measured; zoomed in {zoom}×, and the edges still show at times (try a lighter strength).",
						{ frames: measured, zoom: zoom.zoom.toFixed(2) },
					)
				: zoom
					? t("Holds a shaky camera steady without rewriting the file. {frames} frames measured; zoomed in {zoom}× to hide the edges.", {
							frames: measured,
							zoom: zoom.zoom.toFixed(2),
						})
					: t("Holds a shaky camera steady without rewriting the file. {frames} frames measured.", { frames: measured });

	return (
		<Section sectionKey={`${element.id}:stabilise`} collapsible defaultOpen={current !== "off"}>
			<SectionHeader>
				<SectionTitle>{t("Stabilise")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="stabilise-section">
					<p className="text-muted-foreground text-xs" data-testid="stabilise-note">
						{note}
					</p>
					<div className="flex gap-1">
						{STABILISE_STRENGTHS.map((id) => (
							<Button
								key={id}
								size="sm"
								variant="outline"
								disabled={busy !== null}
								className={cn("flex-1 px-1", current === id && "border-primary text-primary")}
								onClick={() => void pick(id)}
								data-testid={`stabilise-${id}`}
							>
								{label(id)}
							</Button>
						))}
					</div>
					{busy !== null && (
						<p className="text-muted-foreground text-xs" role="status">
							{t("Measuring the camera's movement, frame by frame…")} {Math.round(busy * 100)}%
						</p>
					)}
					{busy === null && status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="stabilise-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

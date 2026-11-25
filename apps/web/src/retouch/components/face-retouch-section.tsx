"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { ImageElement, VideoElement } from "@/timeline/types";
import { blurFaces, retouchFaces } from "../faces";
import { faceTrackFor, faceTrackFound } from "../track";

/**
 * Faces of a video or picture clip: find them, then its Retouch effect
 * smooths and brightens the faces only, or its Face blur hides them
 * (`@/retouch/faces`).
 */
export function FaceRetouchSection({ element, trackId }: { element: VideoElement | ImageElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState<{ blur: boolean; fraction: number } | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const track = faceTrackFor(element.params, element.mediaId);
	const found = track ? faceTrackFound(track) : null;

	const run = async (blur: boolean) => {
		setBusy({ blur, fraction: 0 });
		setStatus(null);
		try {
			const apply = blur ? blurFaces : retouchFaces;
			const result = await apply({ editor, trackId, elementId: element.id, onProgress: (fraction) => setBusy({ blur, fraction }) });
			setStatus(
				blur
					? t("Blurring faces: found in {found} of {looks} looks.", { found: result.found, looks: result.looks })
					: t("Retouching faces: found in {found} of {looks} looks.", { found: result.found, looks: result.looks }),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Finding faces failed"));
		} finally {
			setBusy(null);
		}
	};

	const label = (blur: boolean) =>
		busy?.blur === blur
			? t("Finding faces… {percent}%", { percent: Math.round(busy.fraction * 100) })
			: blur
				? t("Blur faces")
				: t("Retouch faces");

	return (
		<Section sectionKey={`${element.id}:face-retouch`} collapsible defaultOpen={found !== null}>
			<SectionHeader>
				<SectionTitle>{t("Faces")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="face-retouch-section">
					<p className="text-muted-foreground text-xs">
						{found === null
							? t("Retouch smooths and brightens faces only; blur hides them for privacy. Faces are found on this device (MediaPipe, about 10 MB the first time).")
							: t("Retouch smooths and brightens faces only; blur hides them for privacy. Faces found in {found} looks; find them again after changing the clip.", { found })}
					</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" variant="outline" className="flex-1" disabled={busy !== null} onClick={() => void run(false)} data-testid="face-retouch-run">
							{label(false)}
						</Button>
						<Button size="sm" variant="outline" className="flex-1" disabled={busy !== null} onClick={() => void run(true)} data-testid="face-blur-run">
							{label(true)}
						</Button>
					</div>
					{status && busy === null && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

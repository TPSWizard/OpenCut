"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { ImageElement, VideoElement } from "@/timeline/types";
import { retouchFaces } from "../faces";
import { faceTrackFor, faceTrackFound } from "../track";

/**
 * Face-only retouch for a video or picture clip: find its faces, then its
 * Retouch effect smooths and brightens the faces only (`@/retouch/faces`).
 */
export function FaceRetouchSection({ element, trackId }: { element: VideoElement | ImageElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState<number | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const track = faceTrackFor(element.params, element.mediaId);
	const found = track ? faceTrackFound(track) : null;

	const run = async () => {
		setBusy(0);
		setStatus(null);
		try {
			const result = await retouchFaces({ editor, trackId, elementId: element.id, onProgress: (fraction) => setBusy(fraction) });
			setStatus(t("Retouching faces: found in {found} of {looks} looks.", { found: result.found, looks: result.looks }));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Finding faces failed"));
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey={`${element.id}:face-retouch`} collapsible defaultOpen={found !== null}>
			<SectionHeader>
				<SectionTitle>{t("Face retouch")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="face-retouch-section">
					<p className="text-muted-foreground text-xs">
						{found === null
							? t("Smooths and brightens faces only, not hands or the background. Faces are found on this device (MediaPipe, about 10 MB the first time).")
							: t("Smooths and brightens faces only, not hands or the background. Faces found in {found} looks; find them again after changing the clip.", { found })}
					</p>
					<Button size="sm" variant="outline" disabled={busy !== null} onClick={() => void run()} data-testid="face-retouch-run">
						{busy !== null ? t("Finding faces… {percent}%", { percent: Math.round(busy * 100) }) : t("Retouch faces")}
					</Button>
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

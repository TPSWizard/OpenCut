"use client";

import { useState } from "react";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { ImageElement, VideoElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { autoZoomClip, clearAutoZoom } from "../apply";
import { ZOOM_FOCUS_X_PARAM, ZOOM_FOCUS_Y_PARAM, ZOOM_PUNCHES_PARAM, ZOOM_RAMP_PARAM, ZOOM_STRENGTHS, parsePunches, type ZoomStrength } from "../zoom";

const ZOOM_PARAM_KEYS = [ZOOM_PUNCHES_PARAM, ZOOM_FOCUS_X_PARAM, ZOOM_FOCUS_Y_PARAM, ZOOM_RAMP_PARAM] as const;

/**
 * Auto zoom for a video or picture clip: punch-ins at sentence starts and
 * on emphasis from the captions over it, zoomed into the speaker's face.
 */
export function AutoZoomSection({ element, trackId }: { element: VideoElement | ImageElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [strength, setStrength] = useState<ZoomStrength>("medium");
	const [findFace, setFindFace] = useState(element.type === "video");
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const punches = parsePunches(element.params[ZOOM_PUNCHES_PARAM]);
	const strengthLabel = (id: ZoomStrength) => (id === "subtle" ? t("Subtle") : id === "medium" ? t("Medium") : t("Strong"));

	const run = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await autoZoomClip({ editor, trackId, elementId: element.id, strength, findFace: findFace && element.type === "video" });
			setStatus(
				t("{count} punch-in(s) from {words} caption words.", { count: result.punches.length, words: result.words }) +
					(findFace && element.type === "video"
						? ` ${result.focus ? t("Zooming into the face.") : t("No face found; zooming a little above the middle.")}`
						: ""),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Auto zoom failed"));
		} finally {
			setBusy(false);
		}
	};

	return (
		<Section sectionKey={`${element.id}:auto-zoom`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>{t("Auto zoom")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="auto-zoom-section">
					<p className="text-muted-foreground text-xs">
						{t("Small zooms in and out on the speaker at sentence starts, bigger ones on keywords, numbers and exclamations, from the captions over the clip.")}
					</p>
					<div className="flex gap-1">
						{ZOOM_STRENGTHS.map((id) => (
							<Button
								key={id}
								size="sm"
								variant="outline"
								className={cn("flex-1", strength === id && "border-primary text-primary")}
								onClick={() => setStrength(id)}
								data-testid={`auto-zoom-${id}`}
							>
								{strengthLabel(id)}
							</Button>
						))}
					</div>
					{element.type === "video" && (
						<label className="flex items-center gap-2 text-xs">
							<Checkbox checked={findFace} onCheckedChange={(value) => setFindFace(value === true)} data-testid="auto-zoom-face" />
							{t("Zoom into the speaker's face")}
						</label>
					)}
					<div className="flex gap-2">
						<Button size="sm" disabled={busy} onClick={() => void run()} data-testid="auto-zoom-run">
							{busy ? t("Planning zooms…") : t("Auto zoom")}
						</Button>
						<Button
							size="sm"
							variant="ghost"
							disabled={busy || punches.length === 0}
							onClick={() => {
								clearAutoZoom({ editor, trackId, elementId: element.id });
								setStatus(null);
							}}
							data-testid="auto-zoom-clear"
						>
							{t("Remove zooms")}
						</Button>
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
					{punches.length > 0 && (
						<ElementParamsTab element={element} trackId={trackId} paramKeys={ZOOM_PARAM_KEYS} sectionKey="auto-zoom" />
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

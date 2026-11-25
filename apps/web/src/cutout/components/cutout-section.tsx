"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VideoElement } from "@/timeline/types";
import {
	CUTOUT_BACKGROUND_BLUR_DEFAULT,
	clearCutout,
	removeBackground,
	setCutoutBackgroundBlur,
	setCutoutInverted,
} from "../remove";

/** Background removal for a video clip: a person cut-out from an on-device matte. */
export function CutoutSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const matteName = useEditor(
		(e) => e.media.getAssets().find((asset) => asset.id === element.cutout?.matteId)?.name ?? null,
	);
	const [busy, setBusy] = useState(false);
	const [progress, setProgress] = useState(0);
	const [status, setStatus] = useState<string | null>(null);
	const cutout = element.cutout;

	const run = async () => {
		setBusy(true);
		setStatus(null);
		setProgress(0);
		try {
			const result = await removeBackground({ editor, trackId, elementId: element.id, onProgress: setProgress });
			setStatus(
				t("Background removed: {frames} frames; the person covers {share}% of the picture on average.", {
					frames: result.frames,
					share: Math.round(result.keptShare * 100),
				}),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Background removal failed"));
		} finally {
			setBusy(false);
		}
	};

	return (
		<Section sectionKey={`${element.id}:cutout`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>{t("Remove background")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="cutout-section">
					<p className="text-muted-foreground text-xs">
						{t("Keeps the people in the clip and hides the rest. Runs on this device (MediaPipe Selfie Segmenter, about 10 MB the first time) and works best with one or two people facing the camera.")}
					</p>
					{cutout ? (
						<>
							<p className="text-xs" data-testid="cutout-matte">
								{matteName
									? t("Matte: {name}", { name: matteName })
									: t("The matte is missing from the project, so the whole clip shows.")}
							</p>
							<label className="flex items-center justify-between gap-2 text-xs">
								<span>{t("Keep the background instead")}</span>
								<Switch
									checked={Boolean(cutout.inverted)}
									onCheckedChange={(inverted) =>
										setCutoutInverted({ editor, trackId, elementId: element.id, inverted })
									}
								/>
							</label>
							<label className="flex items-center justify-between gap-2 text-xs">
								<span>
									{cutout.inverted
										? t("Blur the person instead of hiding them")
										: t("Blur the background instead of hiding it")}
								</span>
								<Switch
									checked={(cutout.backgroundBlur ?? 0) > 0}
									data-testid="cutout-blur-switch"
									onCheckedChange={(on) =>
										setCutoutBackgroundBlur({
											editor,
											trackId,
											elementId: element.id,
											blur: on ? CUTOUT_BACKGROUND_BLUR_DEFAULT : 0,
										})
									}
								/>
							</label>
							{(cutout.backgroundBlur ?? 0) > 0 && (
								<label className="flex items-center gap-2 text-xs">
									<span className="shrink-0">{t("Blur amount")}</span>
									<input
										type="range"
										min={1}
										max={100}
										step={1}
										className="min-w-0 flex-1"
										value={cutout.backgroundBlur}
										data-testid="cutout-blur-amount"
										onChange={(event) =>
											setCutoutBackgroundBlur({
												editor,
												trackId,
												elementId: element.id,
												blur: Number(event.target.value),
											})
										}
									/>
									<span className="w-7 text-right font-mono">{Math.round(cutout.backgroundBlur ?? 0)}</span>
								</label>
							)}
						</>
					) : null}
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={run} disabled={busy} data-testid="cutout-run">
							{busy
								? t("Removing {percent}%", { percent: Math.round(progress * 100) })
								: cutout
									? t("Redo background removal")
									: t("Remove background")}
						</Button>
						{cutout ? (
							<Button
								size="sm"
								variant="outline"
								disabled={busy}
								onClick={() => {
									clearCutout({ editor, trackId, elementId: element.id });
									setStatus(t("The whole clip shows again; the matte stays in the media."));
								}}
							>
								{t("Show whole clip")}
							</Button>
						) : null}
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="cutout-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

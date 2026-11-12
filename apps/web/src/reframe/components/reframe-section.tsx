"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VideoElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { REFRAME_CAMERAS, isNoopGeometry, reframeGeometry, type ReframeCamera } from "../path";
import { REFRAME_DEFAULTS, clearReframe, reframeClip } from "../scan";

/**
 * Auto-reframe for a video clip: scales it to cover the canvas and pans to
 * keep faces (or, failing those, movement) in view, shot by shot.
 */
export function ReframeSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const canvas = useEditor((e) => e.project.getActive()?.settings.canvasSize);
	const asset = useEditor((e) => e.media.getAssets().find((candidate) => candidate.id === element.mediaId));
	const [camera, setCamera] = useState<ReframeCamera>(REFRAME_DEFAULTS.camera);
	const [resetAtShots, setResetAtShots] = useState(REFRAME_DEFAULTS.resetAtShots);
	const [faces, setFaces] = useState(REFRAME_DEFAULTS.faces);
	const [busy, setBusy] = useState(false);
	const [progress, setProgress] = useState(0);
	const [status, setStatus] = useState<string | null>(null);

	const sameShape =
		canvas && asset?.width && asset.height
			? isNoopGeometry({
					geometry: reframeGeometry({
						sourceWidth: asset.width,
						sourceHeight: asset.height,
						canvasWidth: canvas.width,
						canvasHeight: canvas.height,
					}),
				})
			: false;

	const cameraLabel = (id: ReframeCamera) =>
		id === "steady" ? t("Steady") : id === "smooth" ? t("Smooth") : t("Follow");

	const run = async () => {
		setBusy(true);
		setStatus(null);
		setProgress(0);
		try {
			const result = await reframeClip({
				editor,
				trackId,
				elementId: element.id,
				options: { camera, resetAtShots, faces },
				onProgress: setProgress,
			});
			const lines = [
				t("{faces} of {samples} samples had a face · {shots} shot(s) · {keys} keyframes", {
					faces: result.faceSamples,
					samples: result.samples.length,
					shots: result.plan.shots,
					keys: result.keyframes,
				}),
			];
			if (result.fallbackShots > 0) {
				lines.push(
					t("{count} shot(s) without a face were framed by movement and skin tone.", {
						count: result.fallbackShots,
					}),
				);
			}
			setStatus(lines.join(" "));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Auto reframe failed"));
		} finally {
			setBusy(false);
		}
	};

	const setPortrait = () => {
		void editor.project.updateSettings({
			settings: { canvasSize: { width: 1080, height: 1920 }, canvasSizeMode: "preset" },
		});
	};

	return (
		<Section sectionKey={`${element.id}:reframe`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>{t("Auto reframe")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="reframe-section">
					{sameShape ? (
						<div className="flex flex-col gap-2 rounded-md border border-amber-500/40 bg-amber-500/5 p-2 text-xs">
							<span>{t("The canvas has the clip's shape, so there is nothing to reframe. Change the canvas first.")}</span>
							<Button size="sm" variant="secondary" onClick={setPortrait} data-testid="reframe-portrait">
								{t("Set canvas to 9:16 (1080×1920)")}
							</Button>
						</div>
					) : null}
					<div className="flex flex-col gap-1.5">
						<Label>{t("Camera")}</Label>
						<div className="flex gap-1" role="radiogroup">
							{REFRAME_CAMERAS.map((id) => (
								<button
									key={id}
									type="button"
									role="radio"
									aria-checked={camera === id}
									data-testid={`reframe-camera-${id}`}
									className={cn(
										"rounded-md border px-2 py-1 text-xs",
										camera === id ? "bg-accent border-foreground/30" : "text-muted-foreground",
									)}
									onClick={() => setCamera(id)}
								>
									{cameraLabel(id)}
								</button>
							))}
						</div>
						<p className="text-muted-foreground text-xs">
							{camera === "steady"
								? t("One fixed crop per shot.")
								: camera === "smooth"
									? t("Holds still while the subject does, pans gently when it moves.")
									: t("Pans sooner to keep up with fast movement.")}
						</p>
					</div>
					<label className="flex items-center justify-between gap-2 text-xs">
						<span>{t("New framing at each shot change")}</span>
						<Switch checked={resetAtShots} onCheckedChange={setResetAtShots} />
					</label>
					<label className="flex items-center justify-between gap-2 text-xs">
						<span>{t("Find faces")}</span>
						<Switch checked={faces} onCheckedChange={setFaces} />
					</label>
					<p className="text-muted-foreground text-xs">
						{faces
							? t("Faces are found on this device (MediaPipe, about 10 MB the first time). Shots without a face follow movement and skin tone.")
							: t("Follows movement and skin tone only.")}
					</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={run} disabled={busy || sameShape} data-testid="reframe-run">
							{busy ? t("Analysing {percent}%", { percent: Math.round(progress * 100) }) : t("Reframe")}
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={busy}
							onClick={() => {
								clearReframe({ editor, trackId, elementId: element.id });
								setStatus(t("The clip fits the canvas again."));
							}}
						>
							{t("Reset framing")}
						</Button>
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="reframe-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

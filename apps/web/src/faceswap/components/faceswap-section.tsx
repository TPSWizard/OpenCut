"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { rememberedPasscode, rememberPasscode } from "@/project/share";
import type { ImageElement, VideoElement } from "@/timeline/types";
import { MAX_SWAP_SECONDS, originalOf, restoreOriginal, sourceSpanSeconds, swapFace } from "../apply";
import type { FaceSwapQuality, FaceSwapStage } from "../client";

/**
 * Face swap for a video or picture clip, through the server's Segmind
 * account (`apps/api/src/faceswap.ts`): pick a face photo, confirm you may
 * use it, and the clip shows the swapped version (undo, or "Restore
 * original", puts it back).
 */
export function FaceSwapSection({ element, trackId }: { element: VideoElement | ImageElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const assets = useEditor((e) => e.media.getAssets());
	const { swapped } = useMemo(() => originalOf({ element, assets }), [element, assets]);
	const [face, setFace] = useState<File | null>(null);
	const [preview, setPreview] = useState<string | null>(null);
	const [faceNumber, setFaceNumber] = useState(1);
	const [quality, setQuality] = useState<FaceSwapQuality>("best");
	const [passcode, setPasscode] = useState(() => rememberedPasscode());
	const [consent, setConsent] = useState(false);
	const [stage, setStage] = useState<FaceSwapStage | "preparing" | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const abort = useRef<AbortController | null>(null);

	useEffect(() => {
		if (!face) {
			setPreview(null);
			return;
		}
		const url = URL.createObjectURL(face);
		setPreview(url);
		return () => URL.revokeObjectURL(url);
	}, [face]);

	const seconds = element.type === "video" ? sourceSpanSeconds(element) : null;
	const tooLong = seconds !== null && seconds > MAX_SWAP_SECONDS;
	const busy = stage !== null;

	const stageText: Record<FaceSwapStage | "preparing", string> = {
		preparing: t("Preparing the clip…"),
		uploading: t("Uploading the clip and the face…"),
		queued: t("Waiting for a free GPU…"),
		processing: t("Swapping the face…"),
		downloading: t("Downloading the result…"),
	};

	const run = async () => {
		if (!face) return;
		const controller = new AbortController();
		abort.current = controller;
		setStatus(null);
		try {
			rememberPasscode({ passcode });
			const result = await swapFace({
				editor,
				trackId,
				elementId: element.id,
				face,
				faceIndex: faceNumber - 1,
				quality,
				passcode,
				onStage: setStage,
				signal: controller.signal,
			});
			setStatus(
				result.audioSeparated
					? t("Face swapped. The result had no sound, so the clip's sound is now on its own audio clip.")
					: t("Face swapped. Undo or Restore original puts the original back."),
			);
			setConsent(false);
		} catch (error) {
			setStatus(
				error instanceof DOMException && error.name === "AbortError"
					? t("Stopped. A swap already sent may still be charged.")
					: error instanceof Error
						? error.message
						: t("The face swap failed"),
			);
		} finally {
			setStage(null);
			abort.current = null;
		}
	};

	return (
		<Section sectionKey={`${element.id}:faceswap`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>{t("Face swap")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3 text-xs" data-testid="faceswap-section">
					<p className="text-muted-foreground">
						{t("Puts the face from a photo onto a face in this clip. It runs on Segmind's servers (HyperSwap by FaceFusion Labs): the clip and the photo are uploaded and deleted when the swap ends, and each swap spends the server's Segmind credit. Exports say the video contains a swapped face.")}
					</p>
					{swapped && (
						<div className="flex items-center justify-between gap-2" data-testid="faceswap-swapped">
							<span>{t("This clip shows a face swap.")}</span>
							<Button
								size="sm"
								variant="outline"
								disabled={busy}
								onClick={() => {
									restoreOriginal({ editor, trackId, elementId: element.id });
									setStatus(null);
								}}
							>
								{t("Restore original")}
							</Button>
						</div>
					)}
					<label className="flex flex-col gap-1">
						<span>{t("Face photo")}</span>
						<div className="flex items-center gap-2">
							{preview && <img src={preview} alt="" className="size-10 rounded object-cover" />}
							<Input
								type="file"
								accept="image/png,image/jpeg,image/webp,image/*"
								data-testid="faceswap-face"
								disabled={busy}
								onChange={(event) => setFace(event.target.files?.[0] ?? null)}
							/>
						</div>
					</label>
					<div className="grid grid-cols-2 gap-2">
						<label className="flex flex-col gap-1">
							<span>{t("Face in the clip")}</span>
							<select
								className="bg-background h-8 rounded-md border px-2"
								value={faceNumber}
								disabled={busy}
								data-testid="faceswap-index"
								onChange={(event) => setFaceNumber(Number(event.target.value))}
							>
								{Array.from({ length: 11 }, (_, index) => index + 1).map((number) => (
									<option key={number} value={number}>
										{t("Face {number}", { number })}
									</option>
								))}
							</select>
						</label>
						<label className="flex flex-col gap-1">
							<span>{t("Quality")}</span>
							<select
								className="bg-background h-8 rounded-md border px-2"
								value={quality}
								disabled={busy}
								data-testid="faceswap-quality"
								onChange={(event) => setQuality(event.target.value as FaceSwapQuality)}
							>
								<option value="best">{t("Best")}</option>
								<option value="fast">{t("Fast")}</option>
							</select>
						</label>
					</div>
					<p className="text-muted-foreground">{t("Face 1 is the first face the model finds; try another number if the wrong person changes.")}</p>
					<label className="flex flex-col gap-1">
						<span>{t("Server passcode")}</span>
						<Input
							type="password"
							autoComplete="off"
							value={passcode}
							disabled={busy}
							data-testid="faceswap-passcode"
							onChange={(event) => setPasscode(event.target.value)}
						/>
					</label>
					<label className="flex items-start gap-2" data-testid="faceswap-consent">
						<Checkbox
							checked={consent}
							disabled={busy}
							onCheckedChange={(checked) => setConsent(checked === true)}
						/>
						<span>{t("This face is mine, or I have the person's permission to use it, and the people in the clip have agreed too.")}</span>
					</label>
					{tooLong && (
						<p className="text-destructive">
							{t("Face swap takes clips up to {seconds} seconds; split this one first.", { seconds: MAX_SWAP_SECONDS })}
						</p>
					)}
					<div className="flex gap-2">
						<Button
							size="sm"
							data-testid="faceswap-run"
							disabled={busy || !face || !consent || !passcode || tooLong}
							onClick={run}
						>
							{t("Swap face")}
						</Button>
						{busy && (
							<Button size="sm" variant="outline" onClick={() => abort.current?.abort()}>
								{t("Stop")}
							</Button>
						)}
					</div>
					{stage && <p data-testid="faceswap-stage">{stageText[stage]}</p>}
					{status && <p data-testid="faceswap-status">{status}</p>}
				</div>
			</SectionContent>
		</Section>
	);
}

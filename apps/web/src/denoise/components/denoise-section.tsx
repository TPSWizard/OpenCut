"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Section,
	SectionContent,
	SectionHeader,
	SectionTitle,
} from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import type { AudioCapableElement } from "@/timeline/audio-state";
import { reduceNoiseOnElement } from "../apply";

const REDUCTIONS = [
	{ value: "6", label: "Light (6 dB)" },
	{ value: "12", label: "Normal (12 dB)" },
	{ value: "20", label: "Strong (20 dB)" },
	{ value: "30", label: "Maximum (30 dB)" },
];

/**
 * Noise reduction for a clip: writes a denoised copy of its audio as a new
 * media asset and points the clip at it (a video clip's audio is
 * separated first).
 */
export function DenoiseSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const [reduction, setReduction] = useState("12");
	const [busy, setBusy] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const supported = element.type === "video" || (element.type === "audio" && element.sourceType === "upload");

	const run = async () => {
		setBusy("Starting…");
		setStatus(null);
		try {
			const result = await reduceNoiseOnElement({
				editor,
				trackId,
				elementId: element.id,
				options: { reductionDb: Number(reduction) },
				onProgress: (step) => setBusy(step === "decoding" ? "Decoding…" : step === "denoising" ? "Reducing noise…" : "Saving…"),
			});
			setStatus(
				`Saved ${result.assetName} (${result.seconds.toFixed(1)} s, ${result.channels} ch); the clip now plays it.` +
					(element.type === "video" ? " The audio was separated onto its own track." : ""),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Noise reduction failed");
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey={`${element.id}:denoise`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Noise reduction</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="denoise-section">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="denoise-reduction">Reduction</Label>
						<Select value={reduction} onValueChange={setReduction}>
							<SelectTrigger id="denoise-reduction" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{REDUCTIONS.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<p className="text-muted-foreground text-xs">
						Measures the clip's steady background (hiss, hum, room tone) and subtracts it,
						writing a denoised copy as a new media asset. Undo puts the original back.
					</p>
					<Button size="sm" onClick={run} disabled={busy !== null || !supported}>
						{busy ?? "Reduce noise"}
					</Button>
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

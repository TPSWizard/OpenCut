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
import { isRippleScope, type RippleScope } from "@/silence/detect";
import type { AudioCapableElement } from "@/timeline/audio-state";
import { FILLER_LANGUAGES } from "../detect";
import { cutIntervals, findClipFillers, type FillerReport } from "../remove";

const RIPPLE_LABELS: Record<RippleScope, string> = {
	all: "Close gaps on every track",
	track: "Close gaps on this track",
	none: "Leave gaps",
};

/**
 * Filler-word removal for a clip: finds "um", "uh" and friends in the
 * clip's captions (or a fresh transcription) and cuts them out softly.
 */
export function FillersSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const [language, setLanguage] = useState("auto");
	const [ripple, setRipple] = useState<RippleScope>("all");
	const [report, setReport] = useState<FillerReport | null>(null);
	const [busy, setBusy] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);

	const find = async () => {
		setBusy("Looking…");
		setStatus(null);
		try {
			const result = await findClipFillers({ editor, trackId, elementId: element.id, language, onStep: setBusy });
			setReport(result);
			setStatus(
				result.words === 0
					? "No words found in this clip."
					: `${result.fillers.length} filler${result.fillers.length === 1 ? "" : "s"} in ${result.words} words (${result.source === "captions" ? "from the captions" : "transcribed"}), ${result.seconds.toFixed(2)} s.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not look for fillers");
		} finally {
			setBusy(null);
		}
	};

	const remove = () => {
		if (!report || report.intervals.length === 0) return;
		const pieces = cutIntervals({ editor, trackId, elementId: element.id, intervals: report.intervals, ripple });
		setStatus(`${report.intervals.length} cut${report.intervals.length === 1 ? "" : "s"} made; ${pieces.length} piece${pieces.length === 1 ? "" : "s"} left.`);
		setReport(null);
	};

	return (
		<Section sectionKey={`${element.id}:fillers`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Filler words</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="fillers-section">
					<div className="grid grid-cols-2 gap-2">
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="fillers-language">Language</Label>
							<Select value={language} onValueChange={setLanguage}>
								<SelectTrigger id="fillers-language" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{FILLER_LANGUAGES.map((option) => (
										<SelectItem key={option.id} value={option.id}>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="fillers-ripple">After cutting</Label>
							<Select value={ripple} onValueChange={(value) => isRippleScope(value) && setRipple(value)}>
								<SelectTrigger id="fillers-ripple" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{(Object.keys(RIPPLE_LABELS) as RippleScope[]).map((scope) => (
										<SelectItem key={scope} value={scope}>
											{RIPPLE_LABELS[scope]}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					</div>
					<p className="text-muted-foreground text-xs">
						Uses the word timings of captions over this clip, or transcribes the scene. Each
						filler is cut with 40 ms of padding, never into the neighbouring words.
					</p>
					<div className="flex gap-2">
						<Button size="sm" variant="outline" onClick={find} disabled={busy !== null}>
							{busy ?? "Find fillers"}
						</Button>
						<Button size="sm" onClick={remove} disabled={busy !== null || !report || report.intervals.length === 0}>
							Remove
						</Button>
					</div>
					{report && report.fillers.length > 0 && (
						<p className="text-muted-foreground text-xs" data-testid="fillers-found">
							{report.fillers.map((hit) => `${hit.text.trim()} @ ${hit.start.toFixed(2)} s`).join(", ")}
						</p>
					)}
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

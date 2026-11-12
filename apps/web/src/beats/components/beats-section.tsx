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
import { analyzeClipBeats, beatTimes, bookmarkBeats, unbookmarkBeats, type ClipBeats } from "../analyze";

const EVERY_OPTIONS = [
	{ value: "1", label: "Every beat" },
	{ value: "2", label: "Every 2nd beat" },
	{ value: "4", label: "Every 4th beat (bars)" },
];

/**
 * Beat markers for a clip: finds the tempo and beats, then places
 * bookmarks on them so cuts and clips snap to the beat.
 */
export function BeatsSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const [every, setEvery] = useState("1");
	const [analysis, setAnalysis] = useState<ClipBeats | null>(null);
	const [placed, setPlaced] = useState<number[]>([]);
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);

	const analyze = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await analyzeClipBeats({ editor, trackId, elementId: element.id });
			setAnalysis(result);
			setStatus(
				result.bpm === null
					? "No steady beat found."
					: `${result.bpm.toFixed(1)} BPM, ${result.beats.length} beats, confidence ${Math.round(result.confidence * 100)}%.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Beat detection failed");
		} finally {
			setBusy(false);
		}
	};

	const place = () => {
		if (!analysis) return;
		const times = beatTimes({ markers: analysis.markers, every: Number(every) });
		const added = bookmarkBeats({ editor, times });
		setPlaced(times);
		setStatus(`${added} beat bookmark${added === 1 ? "" : "s"} added; clips and cuts snap to them.`);
	};

	const remove = () => {
		const removed = unbookmarkBeats({ editor, times: placed });
		setPlaced([]);
		setStatus(`${removed} beat bookmark${removed === 1 ? "" : "s"} removed.`);
	};

	return (
		<Section sectionKey={`${element.id}:beats`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Beat markers</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="beats-section">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="beats-every">Mark</Label>
						<Select value={every} onValueChange={setEvery}>
							<SelectTrigger id="beats-every" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{EVERY_OPTIONS.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<p className="text-muted-foreground text-xs">
						Finds the tempo from the clip's onsets (60–200 BPM) and bookmarks the beats; the
						timeline snaps to bookmarks.
					</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={analyze} disabled={busy}>
							{busy ? "Listening…" : "Find beats"}
						</Button>
						<Button size="sm" variant="outline" disabled={busy || !analysis || analysis.bpm === null} onClick={place}>
							Add bookmarks
						</Button>
						<Button size="sm" variant="outline" disabled={busy || placed.length === 0} onClick={remove}>
							Remove
						</Button>
					</div>
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

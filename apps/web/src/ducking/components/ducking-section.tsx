"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
import { hasAnimatedVolume, type AudioCapableElement } from "@/timeline/audio-state";
import { DUCKING_DEFAULTS } from "../envelope";
import { clearDucking, duckElement, listAudioElements } from "../duck";

const ALL_VOICES = "all";

/**
 * Auto-ducking controls for a music clip: which clips carry the voice, how
 * far the music drops and the level that counts as speech. Writes the
 * clip's volume keyframes; Clear removes them.
 */
export function DuckingSection({
	element,
	trackId,
}: {
	element: AudioCapableElement;
	trackId: string;
}) {
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const others = listAudioElements({ tracks }).filter(
		(candidate) => candidate.element.id !== element.id,
	);
	const [voice, setVoice] = useState<string>(ALL_VOICES);
	const [amountDb, setAmountDb] = useState<number>(DUCKING_DEFAULTS.amountDb);
	const [thresholdDb, setThresholdDb] = useState<number>(
		DUCKING_DEFAULTS.thresholdDb,
	);
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const ducked = hasAnimatedVolume({ element });

	const run = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await duckElement({
				editor,
				trackId,
				elementId: element.id,
				voiceElementIds: voice === ALL_VOICES ? undefined : [voice],
				options: { amountDb, thresholdDb },
			});
			setStatus(
				result.keyframes === 0
					? "No speech found over this clip; nothing written."
					: `${result.intervals.length} speech ${result.intervals.length === 1 ? "passage" : "passages"} (${result.speechSeconds.toFixed(1)} s), ${result.keyframes} volume keys written.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Ducking failed");
		} finally {
			setBusy(false);
		}
	};

	return (
		<Section sectionKey={`${element.id}:ducking`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Duck under voice</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="ducking-section">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="ducking-voice">Voice</Label>
						<Select value={voice} onValueChange={setVoice}>
							<SelectTrigger id="ducking-voice" className="w-full">
								<SelectValue placeholder="All other clips" />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value={ALL_VOICES}>All other audio clips</SelectItem>
								{others.map((candidate) => (
									<SelectItem key={candidate.element.id} value={candidate.element.id}>
										{candidate.element.name}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<div className="grid grid-cols-2 gap-2">
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="ducking-amount">Drop (dB)</Label>
							<Input
								id="ducking-amount"
								type="number"
								step={1}
								max={0}
								min={-60}
								value={amountDb}
								onChange={(event) => {
									const next = Number(event.target.value);
									if (Number.isFinite(next)) setAmountDb(Math.min(0, next));
								}}
							/>
						</div>
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="ducking-threshold">Speech above (dBFS)</Label>
							<Input
								id="ducking-threshold"
								type="number"
								step={1}
								max={0}
								min={-90}
								value={thresholdDb}
								onChange={(event) => {
									const next = Number(event.target.value);
									if (Number.isFinite(next)) setThresholdDb(next);
								}}
							/>
						</div>
					</div>
					<p className="text-muted-foreground text-xs">
						Attack {DUCKING_DEFAULTS.attackSeconds} s, release{" "}
						{DUCKING_DEFAULTS.releaseSeconds} s; gaps under{" "}
						{DUCKING_DEFAULTS.holdSeconds} s stay ducked.
					</p>
					<div className="flex gap-2">
						<Button
							size="sm"
							onClick={run}
							disabled={busy || others.length === 0}
						>
							{busy ? "Listening…" : "Duck"}
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={busy || !ducked}
							onClick={() => {
								clearDucking({ editor, trackId, elementId: element.id });
								setStatus("Volume keys cleared.");
							}}
						>
							Clear
						</Button>
					</div>
					{others.length === 0 && (
						<p className="text-muted-foreground text-xs">
							Add a voice clip to duck under.
						</p>
					)}
					{status && (
						<p className="text-xs" data-testid="ducking-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

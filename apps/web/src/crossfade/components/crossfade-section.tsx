"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { CROSSFADE_MAX_SECONDS, type CrossfadeCurve, getClipCrossfade } from "@/timeline/audio-crossfade";
import type { AudioCapableElement } from "@/timeline/audio-state";
import { findTrackInSceneTracks } from "@/timeline/track-element-update";
import type { TimelineElement } from "@/timeline/types";
import { crossfadeElement, nextCrossfadeClip, removeCrossfade } from "../apply";

/**
 * Crossfade from this clip into the next one on its track, in one step:
 * how long and on which curve. Where the clips have too little spare audio
 * they are trimmed to make room and the clips after move left.
 */
export function CrossfadeSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const track = useEditor((e) => findTrackInSceneTracks({ tracks: e.scenes.getActiveScene().tracks, trackId }));
	const current = getClipCrossfade(element);
	const [seconds, setSeconds] = useState<number>(current.seconds > 0 ? current.seconds : 1);
	const [curve, setCurve] = useState<CrossfadeCurve>(current.curve);
	const [status, setStatus] = useState<string | null>(null);
	const next = track ? nextCrossfadeClip(track.elements as TimelineElement[], element.id) : undefined;

	const apply = () => {
		try {
			const result = crossfadeElement({ editor, trackId, elementId: element.id, seconds, curve });
			setSeconds(result.seconds);
			setStatus(
				`Crossfades into "${next?.name ?? "the next clip"}" over ${result.seconds.toFixed(2)} s (${curve === "equalPower" ? "equal power" : "linear"}).` +
					(result.trimmedSeconds > 0
						? ` Trimmed ${result.trimmedSeconds.toFixed(2)} s off the two clips for spare audio; the clips after moved left to match.`
						: ""),
			);
		} catch (error) {
			setStatus(error instanceof Error ? `${error.message}.` : "Could not crossfade");
		}
	};

	return (
		<Section sectionKey={`${element.id}:crossfade`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Crossfade</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="crossfade-section">
					<div className="grid grid-cols-2 gap-2">
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="crossfade-seconds">Length (s)</Label>
							<Input
								id="crossfade-seconds"
								type="number"
								step={0.1}
								min={0.01}
								max={CROSSFADE_MAX_SECONDS}
								value={seconds}
								onChange={(event) => {
									const value = Number(event.target.value);
									if (Number.isFinite(value)) setSeconds(Math.min(CROSSFADE_MAX_SECONDS, Math.max(0, value)));
								}}
							/>
						</div>
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="crossfade-curve">Curve</Label>
							<Select value={curve} onValueChange={(value) => setCurve(value as CrossfadeCurve)}>
								<SelectTrigger id="crossfade-curve" className="w-full" data-testid="crossfade-curve">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="equalPower">Equal power</SelectItem>
									<SelectItem value="linear">Linear</SelectItem>
								</SelectContent>
							</Select>
						</div>
					</div>
					<p className="text-muted-foreground text-xs">
						{next
							? current.seconds > 0
								? `Crossfades into "${next.name}" over ${current.seconds.toFixed(2)} s (${current.curve === "equalPower" ? "equal power" : "linear"}).`
								: `Joins this clip to "${next.name}", which starts where it ends. Equal power keeps the loudness steady; clips without spare audio are trimmed to make room.`
							: "No audio or video clip starts where this one ends on its track."}
					</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={apply} disabled={!next || seconds <= 0} data-testid="crossfade-apply">
							{current.seconds > 0 ? "Update crossfade" : "Crossfade into next"}
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={current.seconds <= 0}
							onClick={() => {
								try {
									removeCrossfade({ editor, trackId, elementId: element.id });
									setStatus("Crossfade removed.");
								} catch (error) {
									setStatus(error instanceof Error ? error.message : "Could not remove the crossfade");
								}
							}}
						>
							Remove
						</Button>
					</div>
					{status && (
						<p className="text-xs" data-testid="crossfade-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

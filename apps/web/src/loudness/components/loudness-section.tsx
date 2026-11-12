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
import { measureElementLoudness, normalizeElementLoudness } from "../measure";
import {
	DEFAULT_LOUDNESS_TARGET,
	LOUDNESS_TARGETS,
	formatLufs,
	type LoudnessReport,
} from "../r128";

function formatDb(value: number, unit: string): string {
	return Number.isFinite(value) ? `${value.toFixed(1)} ${unit}` : "—";
}

/**
 * EBU R128 loudness of a clip's audio (integrated, range, true peak, the
 * loudest momentary and short-term windows) and a Normalise button that
 * sets the clip's volume to meet a delivery target.
 */
export function LoudnessSection({
	element,
	trackId,
}: {
	element: AudioCapableElement;
	trackId: string;
}) {
	const editor = useEditor();
	const [targetId, setTargetId] = useState(DEFAULT_LOUDNESS_TARGET.id);
	const [report, setReport] = useState<LoudnessReport | null>(null);
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const target = LOUDNESS_TARGETS.find((candidate) => candidate.id === targetId) ?? DEFAULT_LOUDNESS_TARGET;

	const measure = async () => {
		setBusy(true);
		setStatus(null);
		try {
			setReport(await measureElementLoudness({ editor, trackId, elementId: element.id }));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Measurement failed");
		} finally {
			setBusy(false);
		}
	};

	const normalize = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await normalizeElementLoudness({
				editor,
				trackId,
				elementId: element.id,
				targetLufs: target.lufs,
				truePeakDbtp: target.truePeakDbtp,
			});
			setReport(result.report);
			const reached = result.report.integratedLufs + result.volumeDb;
			setStatus(
				`${result.volumeDb >= 0 ? "+" : ""}${result.volumeDb.toFixed(1)} dB applied; now ${reached.toFixed(1)} LUFS` +
					(result.peakLimited ? ` (held back by the ${target.truePeakDbtp} dBTP ceiling)` : "") +
					(result.animatedVolume ? "; volume keyframes still apply on top" : "") +
					".",
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Normalising failed");
		} finally {
			setBusy(false);
		}
	};

	return (
		<Section sectionKey={`${element.id}:loudness`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Loudness</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="loudness-section">
					{report && (
						<dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs" data-testid="loudness-report">
							<dt className="text-muted-foreground">Integrated</dt>
							<dd>{formatLufs(report.integratedLufs)}</dd>
							<dt className="text-muted-foreground">Range</dt>
							<dd>{report.loudnessRangeLu.toFixed(1)} LU</dd>
							<dt className="text-muted-foreground">True peak</dt>
							<dd>{formatDb(report.truePeakDbtp, "dBTP")}</dd>
							<dt className="text-muted-foreground">Max momentary</dt>
							<dd>{formatLufs(report.maxMomentaryLufs)}</dd>
							<dt className="text-muted-foreground">Max short-term</dt>
							<dd>{formatLufs(report.maxShortTermLufs)}</dd>
						</dl>
					)}
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="loudness-target">Target</Label>
						<Select value={targetId} onValueChange={setTargetId}>
							<SelectTrigger id="loudness-target" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{LOUDNESS_TARGETS.map((candidate) => (
									<SelectItem key={candidate.id} value={candidate.id}>
										{candidate.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<p className="text-muted-foreground text-xs">
						Measured on the clip's source after trims, before its volume. Normalise sets the
						volume so the integrated loudness meets the target without the true peak passing
						the ceiling.
					</p>
					<div className="flex gap-2">
						<Button size="sm" variant="outline" onClick={measure} disabled={busy}>
							{busy ? "Measuring…" : "Measure"}
						</Button>
						<Button size="sm" onClick={normalize} disabled={busy}>
							Normalise
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

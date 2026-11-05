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
import type { AudioCapableElement } from "@/timeline/audio-state";
import { SILENCE_DEFAULTS, isRippleScope, type RippleScope } from "../detect";
import { cutSilence, detectSilence, type SilenceReport } from "../cut";

const RIPPLE_LABELS: Record<RippleScope, string> = {
	all: "Close gaps on every track",
	track: "Close gaps on this track",
	none: "Leave gaps",
};

/**
 * Silence auto-cut controls for a clip: the level that counts as silence,
 * how long a pause must be to go, the padding kept around speech and what
 * happens to the rest of the timeline. Find reports; Cut applies in one
 * undo step.
 */
export function SilenceSection({
	element,
	trackId,
}: {
	element: AudioCapableElement;
	trackId: string;
}) {
	const editor = useEditor();
	const [thresholdDb, setThresholdDb] = useState(SILENCE_DEFAULTS.thresholdDb);
	const [minSilenceSeconds, setMinSilenceSeconds] = useState(
		SILENCE_DEFAULTS.minSilenceSeconds,
	);
	const [paddingSeconds, setPaddingSeconds] = useState(SILENCE_DEFAULTS.paddingSeconds);
	const [ripple, setRipple] = useState<RippleScope>("all");
	const [busy, setBusy] = useState(false);
	const [report, setReport] = useState<SilenceReport | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const options = { thresholdDb, minSilenceSeconds, paddingSeconds };

	const describe = ({ found }: { found: SilenceReport }) =>
		found.intervals.length === 0
			? "No silence long enough to cut."
			: `${found.intervals.length} silent ${found.intervals.length === 1 ? "stretch" : "stretches"}, ${found.removedSeconds.toFixed(1)} s of ${found.clipDuration.toFixed(1)} s.`;

	const find = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const found = await detectSilence({
				editor,
				trackId,
				elementId: element.id,
				options,
			});
			setReport(found);
			setStatus(describe({ found }));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not measure the clip");
		} finally {
			setBusy(false);
		}
	};

	const cut = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await cutSilence({
				editor,
				trackId,
				elementId: element.id,
				ripple,
				options,
			});
			setReport(null);
			setStatus(
				result.intervals.length === 0
					? "No silence long enough to cut."
					: `Removed ${result.removedSeconds.toFixed(1)} s in ${result.intervals.length} ${result.intervals.length === 1 ? "cut" : "cuts"}; ${result.pieces.length} ${result.pieces.length === 1 ? "piece" : "pieces"} remain.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not cut the clip");
		} finally {
			setBusy(false);
		}
	};

	const numberField = ({
		id,
		label,
		value,
		step,
		min,
		max,
		onChange,
	}: {
		id: string;
		label: string;
		value: number;
		step: number;
		min: number;
		max: number;
		onChange: (value: number) => void;
	}) => (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={id}>{label}</Label>
			<Input
				id={id}
				type="number"
				step={step}
				min={min}
				max={max}
				value={value}
				onChange={(event) => {
					const next = Number(event.target.value);
					if (Number.isFinite(next)) onChange(Math.min(max, Math.max(min, next)));
				}}
			/>
		</div>
	);

	return (
		<Section sectionKey={`${element.id}:silence`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>Cut silence</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="silence-section">
					<div className="grid grid-cols-3 gap-2">
						{numberField({
							id: "silence-threshold",
							label: "Below (dBFS)",
							value: thresholdDb,
							step: 1,
							min: -90,
							max: 0,
							onChange: setThresholdDb,
						})}
						{numberField({
							id: "silence-min",
							label: "Longer than (s)",
							value: minSilenceSeconds,
							step: 0.1,
							min: 0.1,
							max: 60,
							onChange: setMinSilenceSeconds,
						})}
						{numberField({
							id: "silence-padding",
							label: "Keep (s)",
							value: paddingSeconds,
							step: 0.05,
							min: 0,
							max: 2,
							onChange: setPaddingSeconds,
						})}
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="silence-ripple">Afterwards</Label>
						<Select
							value={ripple}
							onValueChange={(next) => {
								if (isRippleScope(next)) setRipple(next);
							}}
						>
							<SelectTrigger id="silence-ripple" className="w-full">
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
					<div className="flex gap-2">
						<Button size="sm" variant="outline" onClick={find} disabled={busy}>
							{busy ? "Listening…" : "Find"}
						</Button>
						<Button size="sm" onClick={cut} disabled={busy}>
							{report && report.intervals.length > 0
								? `Cut ${report.intervals.length}`
								: "Cut"}
						</Button>
					</div>
					{status && (
						<p className="text-xs" data-testid="silence-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

import { useRef } from "react";
import { useEditor } from "@/editor/use-editor";
import { t, useT } from "@/i18n";
import { NumberField } from "@/components/ui/number-field";
import { Switch } from "@/components/ui/switch";
import { HugeiconsIcon } from "@hugeicons/react";
import { DashboardSpeed02Icon } from "@hugeicons/core-free-icons";
import { buildConstantRetime } from "@/retime";
import {
	DEFAULT_RETIME_RATE,
	MIN_RETIME_RATE,
	MAX_RETIME_RATE,
	clampRetimeRate,
	canMaintainPitch,
} from "@/retime/rate";
import type { AudioElement, RetimeConfig, RetimeRampKey, VideoElement } from "@/timeline";
import { TICKS_PER_SECOND } from "@/wasm";
import { Button } from "@/components/ui/button";
import {
	Section,
	SectionContent,
	SectionField,
	SectionFields,
	SectionHeader,
	SectionTitle,
} from "@/components/section";
import { usePropertyDraft } from "@/components/editor/panels/properties/hooks/use-property-draft";
import {
	formatNumberForDisplay,
	getFractionDigitsForStep,
	snapToStep,
} from "@/utils/math";

const SPEED_STEP = 0.01;
const SPEED_FRACTION_DIGITS = getFractionDigitsForStep({ step: SPEED_STEP });

function rateToDisplay({ rate }: { rate: number }): string {
	return formatNumberForDisplay({
		value: rate,
		fractionDigits: SPEED_FRACTION_DIGITS,
	});
}

function parseSpeedInput({ input }: { input: string }): number | null {
	const parsed = parseFloat(input);
	if (Number.isNaN(parsed)) return null;
	return clampRetimeRate({
		rate: snapToStep({ value: parsed, step: SPEED_STEP }),
	});
}

function buildRetime({
	rate,
	maintainPitch,
	reverse = false,
	ramp,
}: {
	rate: number;
	maintainPitch: boolean;
	reverse?: boolean;
	ramp?: RetimeRampKey[];
}): RetimeConfig | undefined {
	const keys = ramp && ramp.length > 0 ? ramp : undefined;
	if (rate === DEFAULT_RETIME_RATE && !maintainPitch && !reverse && !keys) return undefined;
	return { ...buildConstantRetime({ rate, maintainPitch }), ...(reverse ? { reverse: true } : {}), ...(keys ? { ramp: keys } : {}) };
}

const RAMP_PRESETS: { id: string; label: () => string; keys: (duration: number) => RetimeRampKey[] }[] = [
	{ id: "off", label: () => t("Off"), keys: () => [] },
	{ id: "ease-in", label: () => t("Ease in"), keys: (d) => [{ time: 0, rate: 0.25 }, { time: d, rate: 1 }] },
	{ id: "ease-out", label: () => t("Ease out"), keys: (d) => [{ time: 0, rate: 1 }, { time: d, rate: 0.25 }] },
	{ id: "slow-middle", label: () => t("Slow middle"), keys: (d) => [{ time: 0, rate: 1 }, { time: d / 2, rate: 0.25 }, { time: d, rate: 1 }] },
	{ id: "fast-middle", label: () => t("Fast middle"), keys: (d) => [{ time: 0, rate: 1 }, { time: d / 2, rate: 4 }, { time: d, rate: 1 }] },
];

export function SpeedTab({
	element,
	trackId,
}: {
	element: AudioElement | VideoElement;
	trackId: string;
}) {
	const t = useT();
	const editor = useEditor();
	const rate = clampRetimeRate({
		rate: element.retime?.rate ?? DEFAULT_RETIME_RATE,
	});
	const isPitchPreserveAvailable = canMaintainPitch({ rate });
	const maintainPitch = element.retime?.maintainPitch ?? false;
	const reverse = element.retime?.reverse ?? false;
	const ramp = element.retime?.ramp ?? [];
	const durationSeconds = element.duration / TICKS_PER_SECOND;
	const pendingRateRef = useRef(rate);

	const commitRetime = ({
		rate: nextRate,
		maintainPitch: nextMaintainPitch,
	}: {
		rate: number;
		maintainPitch: boolean;
	}) => {
		editor.timeline.updateElementRetime({
			trackId,
			elementId: element.id,
			retime: buildRetime({ rate: nextRate, maintainPitch: nextMaintainPitch, reverse, ramp }),
		});
	};

	const commitShape = ({ nextReverse, nextRamp }: { nextReverse: boolean; nextRamp: RetimeRampKey[] }) => {
		editor.timeline.updateElementRetime({
			trackId,
			elementId: element.id,
			retime: buildRetime({ rate, maintainPitch, reverse: nextReverse, ramp: nextRamp }),
		});
	};

	const speedDraft = usePropertyDraft({
		displayValue: rateToDisplay({ rate }),
		parse: (input) => parseSpeedInput({ input }),
		onPreview: (nextRate) => {
			pendingRateRef.current = nextRate;
			editor.timeline.previewElements({
				updates: [
					{
						trackId,
						elementId: element.id,
						updates: {
							retime: buildRetime({ rate: nextRate, maintainPitch, reverse, ramp }),
						},
					},
				],
			});
		},
		onCommit: () => {
			commitRetime({ rate: pendingRateRef.current, maintainPitch });
		},
	});

	return (
		<Section collapsible sectionKey={`${element.id}:speed`}>
			<SectionHeader>
				<SectionTitle>{t("Speed")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<SectionFields>
					<SectionField label={t("Speed")}>
						<NumberField
							icon={<HugeiconsIcon icon={DashboardSpeed02Icon} />}
							value={speedDraft.displayValue}
							suffix="x"
							scrubRanges={[
								{ from: 0.01, to: 1, pixelsPerUnit: 160 },
								{ from: 1, to: 5, pixelsPerUnit: 48 },
							]}
							scrubClamp={{ min: MIN_RETIME_RATE, max: MAX_RETIME_RATE }}
							onFocus={() => {
								pendingRateRef.current = rate;
								speedDraft.onFocus();
							}}
							onChange={speedDraft.onChange}
							onBlur={speedDraft.onBlur}
							onScrub={speedDraft.scrubTo}
							onScrubEnd={speedDraft.commitScrub}
							onReset={() =>
								commitRetime({ rate: DEFAULT_RETIME_RATE, maintainPitch })
							}
							isDefault={rate === DEFAULT_RETIME_RATE}
						/>
					</SectionField>
					<SectionField label={t("Ramp")}>
						<div className="flex flex-wrap gap-1" data-testid="speed-ramp">
							{RAMP_PRESETS.map((preset) => {
								const keys = preset.keys(durationSeconds);
								const active =
									keys.length === 0
										? ramp.length === 0
										: ramp.length === keys.length && ramp.every((key, index) => Math.abs(key.rate - keys[index].rate) < 1e-6);
								return (
									<Button
										key={preset.id}
										size="sm"
										variant={active ? "default" : "outline"}
										onClick={() => commitShape({ nextReverse: reverse, nextRamp: keys })}
									>
										{preset.label()}
									</Button>
								);
							})}
						</div>
					</SectionField>
					<div className="flex items-center justify-between">
						<span className="text-sm">{t("Reverse")}</span>
						<Switch
							checked={reverse}
							onCheckedChange={(checked) => commitShape({ nextReverse: checked, nextRamp: ramp })}
						/>
					</div>
					<div className="flex items-center justify-between">
						<span className="text-sm">{t("Change pitch")}</span>
						<Switch
							checked={!maintainPitch}
							disabled={!isPitchPreserveAvailable}
							onCheckedChange={(checked) =>
								commitRetime({ rate, maintainPitch: !checked })
							}
						/>
					</div>
				</SectionFields>
			</SectionContent>
		</Section>
	);
}

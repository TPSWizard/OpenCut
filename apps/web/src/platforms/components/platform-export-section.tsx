"use client";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { useEditor } from "@/editor/use-editor";
import { useEditorStore } from "@/editor/editor-store";
import { floatToFrameRate, frameRateToFloat } from "@/fps/utils";
import {
	PLATFORM_PRESETS,
	checkPlatformExport,
	describeAspect,
	estimatePlatformFileBytes,
	formatBytes,
	getPlatformPreset,
	isPlatformPresetId,
	recommendedVideoBitrate,
	type PlatformIssue,
	type PlatformPresetId,
} from "@/platforms";
import { mediaTimeToSeconds } from "opencut-wasm";
import { cn } from "@/utils/ui";
import type { TCanvasSize } from "@/project/types";
import { useT } from "@/i18n";

export const CUSTOM_PLATFORM = "custom";

export type PlatformChoice = PlatformPresetId | typeof CUSTOM_PLATFORM;

export function isPlatformChoice(value: string): value is PlatformChoice {
	return value === CUSTOM_PLATFORM || isPlatformPresetId(value);
}

/**
 * The platform picker of the export popover, with what the chosen platform
 * would object to in the project as it is and a button for each objection
 * a project setting can clear.
 */
export function PlatformExportSection({
	value,
	onChange,
	includeAudio,
}: {
	value: PlatformChoice;
	onChange: (value: PlatformChoice) => void;
	includeAudio: boolean;
}) {
	const t = useT();
	const editor = useEditor();
	const activeProject = useEditor((e) => e.project.getActive());
	const duration = useEditor((e) => e.timeline.getTotalDuration());
	const { canvasPresets } = useEditorStore();
	const preset = getPlatformPreset({ id: value });
	const canvas = activeProject.settings.canvasSize;
	const fps = frameRateToFloat(activeProject.settings.fps);
	const durationSeconds = mediaTimeToSeconds({ time: duration });
	const issues = preset
		? checkPlatformExport({
				preset,
				canvas,
				fps,
				durationSeconds,
				includeAudio,
			})
		: [];

	const applyFix = ({ fix }: { fix: NonNullable<PlatformIssue["fix"]> }) => {
		const settings: Parameters<typeof editor.project.updateSettings>[0]["settings"] =
			{};
		if (fix.canvas) {
			const nextCanvas: TCanvasSize = { ...fix.canvas };
			const isPreset = canvasPresets.some(
				(candidate) =>
					candidate.width === nextCanvas.width &&
					candidate.height === nextCanvas.height,
			);
			settings.canvasSize = nextCanvas;
			settings.canvasSizeMode = isPreset ? "preset" : "custom";
			if (!isPreset) settings.lastCustomCanvasSize = nextCanvas;
		}
		if (fix.fps !== undefined) {
			settings.fps = floatToFrameRate(fix.fps);
		}
		void editor.project.updateSettings({ settings });
	};

	return (
		<div className="flex flex-col gap-2 px-3 pt-3" data-testid="platform-export">
			<Label htmlFor="export-platform">{t("Platform")}</Label>
			<Select
				value={value}
				onValueChange={(next) => {
					if (isPlatformChoice(next)) onChange(next);
				}}
			>
				<SelectTrigger id="export-platform" className="w-full">
					<SelectValue placeholder={t("Custom")} />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={CUSTOM_PLATFORM}>{t("Custom")}</SelectItem>
					{PLATFORM_PRESETS.map((candidate) => (
						<SelectItem key={candidate.id} value={candidate.id}>
							{candidate.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
			{preset && (
				<div className="flex flex-col gap-1.5 text-xs">
					<p className="text-muted-foreground">
						{(() => {
							const params = {
								bitrate: formatMegabits({
									bits: recommendedVideoBitrate({ canvas, fps }),
								}),
								size: formatBytes({
									bytes: estimatePlatformFileBytes({
										canvas,
										fps,
										durationSeconds,
										includeAudio,
									}),
								}),
								width: preset.canvas.width,
								height: preset.canvas.height,
								aspect: describeAspect({ canvas: preset.canvas }),
								fps: preset.fps,
							};
							return preset.maxDurationSeconds !== null
								? t(
										"MP4, H.264 High profile at {bitrate} with a half-second closed GOP and the index first, about {size}. Target {width}×{height} ({aspect}) at {fps} fps, up to {minutes} min.",
										{ ...params, minutes: Math.round(preset.maxDurationSeconds / 60) },
									)
								: t(
										"MP4, H.264 High profile at {bitrate} with a half-second closed GOP and the index first, about {size}. Target {width}×{height} ({aspect}) at {fps} fps.",
										params,
									);
						})()}
					</p>
					{preset.verified === "third-party" && (
						<p className="text-muted-foreground">
							{t("Limits come from third-party guides, not {platform}'s own pages.", { platform: preset.label })}
						</p>
					)}
					{issues.length === 0 ? (
						<p className="text-constructive" data-testid="platform-ok">
							{t("The project matches {platform}.", { platform: preset.label })}
						</p>
					) : (
						<ul className="flex flex-col gap-1.5" aria-label={t("Platform checks")}>
							{issues.map((issue) => (
								<li
									key={issue.kind}
									className={cn(
										"flex flex-col gap-1",
										issue.severity === "error"
											? "text-destructive"
											: "text-amber-600 dark:text-amber-400",
									)}
								>
									<span>{issue.message}</span>
									{issue.fix && (
										<Button
											variant="outline"
											size="sm"
											className="h-7 w-fit text-xs"
											onClick={() => applyFix({ fix: issue.fix! })}
										>
											{issue.fix.canvas
												? t("Set canvas to {width}×{height}", { width: issue.fix.canvas.width, height: issue.fix.canvas.height })
												: t("Set project to {fps} fps", { fps: String(issue.fix.fps) })}
										</Button>
									)}
								</li>
							))}
						</ul>
					)}
				</div>
			)}
		</div>
	);
}

function formatMegabits({ bits }: { bits: number }): string {
	const megabits = bits / 1_000_000;
	return `${Number.isInteger(megabits) ? megabits : megabits.toFixed(1)} Mbps`;
}

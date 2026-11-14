"use client";

import { useEffect, useState } from "react";
import { TransitionTopIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Progress } from "@/components/ui/progress";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { cn } from "@/utils/ui";
import {
	getExportMimeType,
	getExportFileExtension,
	downloadBuffer,
} from "@/export";
import { Check, Copy, Download, RotateCcw } from "lucide-react";
import {
	EXPORT_FORMAT_VALUES,
	EXPORT_QUALITY_VALUES,
	type ExportFormat,
	type ExportQuality,
} from "@/export";
import {
	Section,
	SectionContent,
	SectionHeader,
	SectionTitle,
} from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { DEFAULT_EXPORT_OPTIONS } from "@/export/defaults";
import {
	CODECS_BY_FORMAT,
	CODEC_LABELS,
	type ExportVideoCodec,
	MAX_BITRATE_MBPS,
	MIN_BITRATE_MBPS,
	encodableCodecs,
	isExportVideoCodec,
	parseBitrateMbps,
	resolveVideoCodec,
} from "@/export/codecs";
import {
	CUSTOM_PLATFORM,
	PlatformExportSection,
	type PlatformChoice,
} from "@/platforms/components/platform-export-section";
import { getPlatformPreset, platformEncodeSettings } from "@/platforms";
import { frameRateToFloat } from "@/fps/utils";
import { useT } from "@/i18n";

function isExportFormat(value: string): value is ExportFormat {
	return EXPORT_FORMAT_VALUES.some((formatValue) => formatValue === value);
}

function isExportQuality(value: string): value is ExportQuality {
	return EXPORT_QUALITY_VALUES.some((qualityValue) => qualityValue === value);
}

export function ExportButton() {
	const t = useT();
	const [isExportPopoverOpen, setIsExportPopoverOpen] = useState(false);
	const editor = useEditor();
	const activeProject = useEditor((e) => e.project.getActiveOrNull());
	const hasProject = !!activeProject;

	const handlePopoverOpenChange = ({ open }: { open: boolean }) => {
		if (!open) {
			editor.project.cancelExport();
			editor.project.clearExportState();
		}
		setIsExportPopoverOpen(open);
	};

	return (
		<Popover
			open={isExportPopoverOpen}
			onOpenChange={(open) => handlePopoverOpenChange({ open })}
		>
			<PopoverTrigger asChild>
				<button
					type="button"
					className={cn(
						"flex items-center gap-1.5 rounded-md bg-[#38BDF8] px-[0.12rem] py-[0.12rem] text-white",
						hasProject ? "cursor-pointer" : "cursor-not-allowed opacity-50",
					)}
					onClick={hasProject ? () => setIsExportPopoverOpen(true) : undefined}
					disabled={!hasProject}
					onKeyDown={(event) => {
						if (hasProject && (event.key === "Enter" || event.key === " ")) {
							event.preventDefault();
							setIsExportPopoverOpen(true);
						}
					}}
				>
					<div className="relative flex items-center gap-1.5 rounded-[0.6rem] bg-linear-270 from-[#2567EC] to-[#37B6F7] px-4 py-1 shadow-[0_1px_3px_0px_rgba(0,0,0,0.65)]">
						<HugeiconsIcon icon={TransitionTopIcon} className="z-50 size-3.5" />
						<span className="z-50 text-[0.875rem]">{t("Export")}</span>
						<div className="absolute top-0 left-0 z-10 flex size-full items-center justify-center rounded-[0.6rem] bg-linear-to-t from-white/0 to-white/50">
							<div className="absolute top-[0.08rem] z-50 h-[calc(100%-2px)] w-[calc(100%-2px)] rounded-[0.6rem] bg-linear-270 from-[#2567EC] to-[#37B6F7]"></div>
						</div>
					</div>
				</button>
			</PopoverTrigger>
			{hasProject && <ExportPopover onOpenChange={setIsExportPopoverOpen} />}
		</Popover>
	);
}

function ExportPopover({
	onOpenChange,
}: {
	onOpenChange: (open: boolean) => void;
}) {
	const t = useT();
	const editor = useEditor();
	const activeProject = useEditor((e) => e.project.getActive());
	const exportState = useEditor((e) => e.project.getExportState());
	const { isExporting, progress, result: exportResult } = exportState;
	const [format, setFormat] = useState<ExportFormat>(
		DEFAULT_EXPORT_OPTIONS.format,
	);
	const [quality, setQuality] = useState<ExportQuality>(
		DEFAULT_EXPORT_OPTIONS.quality,
	);
	const [shouldIncludeAudio, setShouldIncludeAudio] = useState<boolean>(
		DEFAULT_EXPORT_OPTIONS.includeAudio ?? true,
	);
	const [platform, setPlatform] = useState<PlatformChoice>(CUSTOM_PLATFORM);
	const platformPreset = getPlatformPreset({ id: platform });
	const [codecChoice, setCodecChoice] = useState<ExportVideoCodec | null>(null);
	const codec = resolveVideoCodec({ format, codec: codecChoice });
	const [hardware, setHardware] = useState(false);
	const [useBitrate, setUseBitrate] = useState(false);
	const [bitrateText, setBitrateText] = useState("8");
	const videoBitrate = useBitrate ? parseBitrateMbps(bitrateText) : null;
	const bitrateInvalid = useBitrate && videoBitrate === null;
	// What this browser can encode at the canvas size; null while checking.
	const [encodable, setEncodable] = useState<ExportVideoCodec[] | null>(null);
	const canvasWidth = activeProject?.settings.canvasSize.width ?? 1920;
	const canvasHeight = activeProject?.settings.canvasSize.height ?? 1080;
	useEffect(() => {
		let current = true;
		setEncodable(null);
		void encodableCodecs({ format, width: canvasWidth, height: canvasHeight, hardware }).then((codecs) => {
			if (current) setEncodable(codecs);
		});
		return () => {
			current = false;
		};
	}, [format, canvasWidth, canvasHeight, hardware]);
	const codecUnavailable = !platformPreset && encodable !== null && !encodable.includes(codec);

	const handleExport = async () => {
		if (!activeProject) return;

		const exportFormat = platformPreset ? "mp4" : format;
		const result = await editor.project.export({
			options: {
				format: exportFormat,
				quality,
				fps: activeProject.settings.fps,
				includeAudio: shouldIncludeAudio,
				platform: platformPreset
					? platformEncodeSettings({
							canvas: activeProject.settings.canvasSize,
							fps: frameRateToFloat(activeProject.settings.fps),
						})
					: undefined,
				videoCodec: platformPreset ? undefined : codec,
				videoBitrate: platformPreset ? undefined : (videoBitrate ?? undefined),
				hardware: !platformPreset && hardware,
			},
		});

		if (result.cancelled) {
			editor.project.clearExportState();
			return;
		}

		if (result.success && result.buffer) {
			downloadBuffer({
				buffer: result.buffer,
				filename: `${activeProject.metadata.name}${getExportFileExtension({ format: exportFormat })}`,
				mimeType: getExportMimeType({ format: exportFormat }),
			});

			editor.project.clearExportState();
			onOpenChange(false);
		}
	};

	const handleCancel = () => {
		editor.project.cancelExport();
	};

	return (
		<PopoverContent className="bg-background mr-4 flex w-80 flex-col p-0">
			{exportResult && !exportResult.success ? (
				<ExportError
					error={exportResult.error || t("Unknown error occurred")}
					onRetry={handleExport}
				/>
			) : (
				<>
					<div className="flex items-center justify-between p-3 border-b">
						<h3 className="font-medium text-sm">
							{isExporting ? t("Exporting project") : t("Export project")}
						</h3>
					</div>

					<div className="flex flex-col gap-4">
						{!isExporting && (
							<>
								<PlatformExportSection
									value={platform}
									onChange={setPlatform}
									includeAudio={shouldIncludeAudio}
								/>
								<div className="flex flex-col">
									{!platformPreset && (
									<Section
										collapsible
										defaultOpen={false}
										showTopBorder={false}
									>
										<SectionHeader>
											<SectionTitle>{t("Format")}</SectionTitle>
										</SectionHeader>
										<SectionContent>
											<RadioGroup
												value={format}
												onValueChange={(value) => {
													if (isExportFormat(value)) {
														setFormat(value);
													}
												}}
											>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="mp4" id="mp4" />
													<Label htmlFor="mp4">
														{t("MP4 - Plays everywhere")}
													</Label>
												</div>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="webm" id="webm" />
													<Label htmlFor="webm">
														{t("WebM - Open format for the web")}
													</Label>
												</div>
											</RadioGroup>
										</SectionContent>
									</Section>
									)}

									{!platformPreset && (
									<Section collapsible defaultOpen={false}>
										<SectionHeader>
											<SectionTitle>
												{t("Video codec: {codec}", { codec: CODEC_LABELS[codec] })}
											</SectionTitle>
										</SectionHeader>
										<SectionContent>
											<RadioGroup
												value={codec}
												onValueChange={(value) => {
													if (isExportVideoCodec(value)) setCodecChoice(value);
												}}
												data-testid="export-codec"
											>
												{CODECS_BY_FORMAT[format].map((id) => {
													const missing = encodable !== null && !encodable.includes(id);
													return (
														<div key={id} className="flex items-center space-x-2">
															<RadioGroupItem value={id} id={`codec-${id}`} />
															<Label htmlFor={`codec-${id}`} className={cn(missing && "text-muted-foreground")}>
																{CODEC_LABELS[id]}
																{id === "hevc" && ` - ${t("Smaller files, plays on Apple devices")}`}
																{id === "av1" && ` - ${t("Smallest files, slowest to encode")}`}
																{missing && ` (${t("not in this browser")})`}
															</Label>
														</div>
													);
												})}
											</RadioGroup>
											<div className="mt-3 flex items-center space-x-2">
												<Checkbox
													id="export-hardware"
													checked={hardware}
													onCheckedChange={(checked) => setHardware(!!checked)}
												/>
												<Label htmlFor="export-hardware">{t("Prefer the hardware encoder (faster)")}</Label>
											</div>
										</SectionContent>
									</Section>
									)}

									{!platformPreset && (
									<Section collapsible defaultOpen={false}>
										<SectionHeader>
											<SectionTitle>{t("Quality")}</SectionTitle>
										</SectionHeader>
										<SectionContent>
											<RadioGroup
												value={useBitrate ? "bitrate" : quality}
												onValueChange={(value) => {
													if (value === "bitrate") {
														setUseBitrate(true);
													} else if (isExportQuality(value)) {
														setUseBitrate(false);
														setQuality(value);
													}
												}}
											>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="low" id="low" />
													<Label htmlFor="low">{t("Low - Smallest file size")}</Label>
												</div>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="medium" id="medium" />
													<Label htmlFor="medium">{t("Medium - Balanced")}</Label>
												</div>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="high" id="high" />
													<Label htmlFor="high">{t("High - Recommended")}</Label>
												</div>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="very_high" id="very_high" />
													<Label htmlFor="very_high">
														{t("Very high - Largest file size")}
													</Label>
												</div>
												<div className="flex items-center space-x-2">
													<RadioGroupItem value="bitrate" id="bitrate" />
													<Label htmlFor="bitrate">{t("Target bitrate")}</Label>
												</div>
											</RadioGroup>
											{useBitrate && (
												<div className="mt-2 flex flex-col gap-1">
													<div className="flex items-center gap-2">
														<Input
															aria-label={t("Target bitrate in Mbps")}
															inputMode="decimal"
															className="h-8 w-24"
															value={bitrateText}
															onChange={(event) => setBitrateText(event.target.value)}
															data-testid="export-bitrate"
														/>
														<span className="text-muted-foreground text-xs">Mbps</span>
													</div>
													{bitrateInvalid && (
														<p className="text-destructive text-xs">
															{t("Enter a number from {min} to {max}", {
																min: MIN_BITRATE_MBPS,
																max: MAX_BITRATE_MBPS,
															})}
														</p>
													)}
												</div>
											)}
										</SectionContent>
									</Section>
									)}

									<Section collapsible defaultOpen={false}>
										<SectionHeader>
											<SectionTitle>{t("Audio")}</SectionTitle>
										</SectionHeader>
										<SectionContent>
											<div className="flex items-center space-x-2">
												<Checkbox
													id="include-audio"
													checked={shouldIncludeAudio}
													onCheckedChange={(checked) =>
														setShouldIncludeAudio(!!checked)
													}
												/>
												<Label htmlFor="include-audio">
													{t("Include audio in export")}
												</Label>
											</div>
										</SectionContent>
									</Section>
								</div>

								<div className="flex flex-col gap-2 p-3 pt-0">
									{codecUnavailable && (
										<p className="text-destructive text-xs" role="alert">
											{t("This browser cannot encode {codec}. Choose another codec, or export from the desktop app.", {
												codec: CODEC_LABELS[codec],
											})}
										</p>
									)}
									<Button
										onClick={handleExport}
										className="w-full gap-2"
										disabled={(!platformPreset && bitrateInvalid) || codecUnavailable}
									>
										<Download className="size-4" />
										{t("Export")}
									</Button>
								</div>
							</>
						)}

						{isExporting && (
							<div className="space-y-4 p-3">
								<div className="flex flex-col gap-2">
									<div className="flex items-center justify-between text-center">
										<p className="text-muted-foreground text-sm">
											{Math.round(progress * 100)}%
										</p>
										<p className="text-muted-foreground text-sm">100%</p>
									</div>
									<Progress value={progress * 100} className="w-full" />
								</div>

								<Button
									variant="outline"
									className="w-full rounded-md"
									onClick={handleCancel}
								>
									{t("Cancel")}
								</Button>
							</div>
						)}
					</div>
				</>
			)}
		</PopoverContent>
	);
}

function ExportError({
	error,
	onRetry,
}: {
	error: string;
	onRetry: () => void;
}) {
	const t = useT();
	const [copied, setCopied] = useState(false);

	const handleCopy = async () => {
		await navigator.clipboard.writeText(error);
		setCopied(true);
		setTimeout(() => setCopied(false), 1000);
	};

	return (
		<div className="space-y-4 p-3">
			<div className="flex flex-col gap-1.5">
				<p className="text-destructive text-sm font-medium">{t("Export failed")}</p>
				<p className="text-muted-foreground text-xs">{error}</p>
			</div>

			<div className="flex gap-2">
				<Button
					variant="outline"
					size="sm"
					className="h-8 flex-1 text-xs"
					onClick={handleCopy}
				>
					{copied ? <Check className="text-constructive" /> : <Copy />}
					{t("Copy")}
				</Button>
				<Button
					variant="outline"
					size="sm"
					className="h-8 flex-1 text-xs"
					onClick={onRetry}
				>
					<RotateCcw />
					{t("Retry")}
				</Button>
			</div>
		</div>
	);
}

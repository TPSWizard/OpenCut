"use client";

import { useState, useEffect } from "react";
import { useEditor } from "@/editor/use-editor";
import { formatTimecode } from "opencut-wasm";
import { invokeAction } from "@/actions";
import { EditableTimecode } from "@/components/editable-timecode";
import { Button } from "@/components/ui/button";
import {
	FullScreenIcon,
	GridTableIcon,
	PauseIcon,
	PlayIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Separator } from "@/components/ui/separator";
import {
	Select,
	SelectTrigger,
	SelectContent,
	SelectItem,
	SelectSeparator,
} from "@/components/ui/select";
import { PREVIEW_ZOOM_PRESETS } from "@/preview/zoom";
import { usePreviewStore } from "@/preview/preview-store";
import { PREVIEW_QUALITIES, isPreviewQuality } from "@/media/proxy";
import { usePreviewViewport } from "./preview-viewport";
import { GridPopover } from "./guide-popover";
import { getGuideById } from "@/guides";
import type { MediaTime } from "@/wasm";
import { useT } from "@/i18n";

/** Preview quality names (PREVIEW_QUALITIES holds the English; fractions stay as they are). */
function qualityLabel(value: string, fallback: string, t: ReturnType<typeof useT>): string {
	switch (value) {
		case "auto":
			return t("Auto");
		case "full":
			return t("Full");
		default:
			return fallback;
	}
}

export function PreviewToolbar({
	onToggleFullscreen,
}: {
	onToggleFullscreen: () => void;
}) {
	const t = useT();
	const activeGuide = usePreviewStore((state) => state.activeGuide);
	const activeGuideDefinition = getGuideById(activeGuide);
	return (
		<div className="grid grid-cols-[1fr_auto_1fr] items-center pb-3 pt-5 px-5">
			<TimecodeDisplay />
			<PlayPauseButton />
			<div className="justify-self-end flex items-center gap-2.5">
				<QualitySelect />
				<ZoomSelect />
				<Separator orientation="vertical" className="h-4" />
				<GridPopover>
					<Button
						variant={activeGuideDefinition ? "secondary" : "text"}
						size="icon"
						aria-label={t("Guides")}
						title={t("Guides and safe areas")}
					>
						{activeGuideDefinition ? (
							activeGuideDefinition.renderTriggerIcon()
						) : (
							<HugeiconsIcon icon={GridTableIcon} />
						)}
					</Button>
				</GridPopover>
				<Button variant="text" onClick={onToggleFullscreen}>
					<HugeiconsIcon icon={FullScreenIcon} />
				</Button>
			</div>
		</div>
	);
}

function TimecodeDisplay() {
	const editor = useEditor();
	const totalDuration = useEditor((e) => e.timeline.getTotalDuration());
	const fps = useEditor((e) => e.project.getActive().settings.fps);
	const [currentTime, setCurrentTime] = useState<MediaTime>(() =>
		editor.playback.getCurrentTime(),
	);

	useEffect(() => {
		const unsubscribeUpdate = editor.playback.onUpdate(setCurrentTime);
		const unsubscribeSeek = editor.playback.onSeek(setCurrentTime);
		return () => {
			unsubscribeUpdate();
			unsubscribeSeek();
		};
	}, [editor.playback]);

	return (
		<div className="flex items-center">
			<EditableTimecode
				time={currentTime}
				duration={totalDuration}
				format="HH:MM:SS:FF"
				fps={fps}
				onTimeChange={({ time }) => editor.playback.seek({ time })}
				className="text-center"
			/>
			<span className="text-muted-foreground px-2 font-mono text-xs">/</span>
			<span className="text-muted-foreground font-mono text-xs">
				{formatTimecode({
					time: totalDuration,
					format: "HH:MM:SS:FF",
					rate: fps,
				})}
			</span>
		</div>
	);
}

/** Preview decode quality and proxies; export always uses full quality. */
function QualitySelect() {
	const t = useT();
	const quality = usePreviewStore((state) => state.previewQuality);
	const useProxies = usePreviewStore((state) => state.useProxies);
	const autoProxies = usePreviewStore((state) => state.autoProxies);
	const label = qualityLabel(quality, PREVIEW_QUALITIES.find((item) => item.value === quality)?.label ?? "Auto", t);

	const onValueChange = (value: string) => {
		const store = usePreviewStore.getState();
		if (value === "toggle-proxies") store.setUseProxies(!useProxies);
		else if (value === "toggle-auto-proxies") store.setAutoProxies(!autoProxies);
		else if (isPreviewQuality(value)) store.setPreviewQuality(value);
	};

	return (
		<Select value={quality} onValueChange={onValueChange}>
			<SelectTrigger
				className="tabular-nums"
				title={t("Preview quality (export always renders at full quality)")}
				data-testid="preview-quality"
			>
				{useProxies ? label : t("{quality} · originals", { quality: label })}
			</SelectTrigger>
			<SelectContent>
				{PREVIEW_QUALITIES.map((item) => (
					<SelectItem key={item.value} value={item.value}>
						{qualityLabel(item.value, item.label, t)}
					</SelectItem>
				))}
				<SelectSeparator />
				<SelectItem value="toggle-proxies">
					{useProxies ? t("Proxies on (click to use originals)") : t("Proxies off (click to use them)")}
				</SelectItem>
				<SelectItem value="toggle-auto-proxies">
					{autoProxies ? t("Auto proxies above 1080p: on") : t("Auto proxies above 1080p: off")}
				</SelectItem>
			</SelectContent>
		</Select>
	);
}

function ZoomSelect() {
	const t = useT();
	const { isAtFit, zoomPercent, fitToScreen, setViewportPercent } =
		usePreviewViewport();

	const displayLabel = isAtFit ? t("Fit") : `${zoomPercent}%`;

	const onValueChange = (value: string) => {
		if (value === "fit") {
			fitToScreen();
		} else {
			setViewportPercent({ percent: Number(value) });
		}
	};

	return (
		<Select
			value={isAtFit ? "fit" : String(zoomPercent)}
			onValueChange={onValueChange}
		>
			<SelectTrigger className="tabular-nums">{displayLabel}</SelectTrigger>
			<SelectContent>
				<SelectItem value="fit">{t("Fit")}</SelectItem>
				<SelectSeparator />
				{PREVIEW_ZOOM_PRESETS.map((preset) => (
					<SelectItem key={preset} value={String(preset)}>
						{preset}%
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

function PlayPauseButton() {
	const isPlaying = useEditor((e) => e.playback.getIsPlaying());

	return (
		<Button
			variant="text"
			size="icon"
			onClick={() => invokeAction("toggle-play")}
		>
			<HugeiconsIcon icon={isPlaying ? PauseIcon : PlayIcon} />
		</Button>
	);
}

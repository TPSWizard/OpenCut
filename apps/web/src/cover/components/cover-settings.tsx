"use client";

import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { mediaTimeFromSeconds, TICKS_PER_SECOND } from "@/wasm";
import { COVER_SHAPES, COVER_STYLES, type CoverSettings, type CoverShape, type CoverStyle, coverSize } from "../layout";
import { coverOf, renderCoverPng, suggestCoverTime } from "../render";

const STYLE_LABELS: Record<CoverStyle, string> = { bold: "Bold", band: "Band", top: "Top" };
const SHAPE_LABELS: Record<CoverShape, string> = { canvas: "Canvas", landscape: "16:9", portrait: "9:16" };

function clock(seconds: number): string {
	const whole = Math.max(0, seconds);
	const minutes = Math.floor(whole / 60);
	return `${minutes}:${(whole - minutes * 60).toFixed(1).padStart(4, "0")}`;
}

function fileName(name: string, width: number, height: number): string {
	return `${name.replace(/[\\/:*?"<>|]+/g, "_").trim() || "cover"} cover ${width}x${height}.png`;
}

/**
 * The cover maker: a frame and a title, previewed, saved as a PNG at the
 * canvas shape, 16:9 or 9:16, and embedded as the cover of MP4 exports.
 */
export function CoverSettingsView() {
	const t = useT();
	const editor = useEditor();
	const project = useEditor((e) => e.project.getActive());
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const cover = coverOf(project.settings);
	const [titleDraft, setTitleDraft] = useState(cover.title);
	const [preview, setPreview] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const previewUrl = useRef<string | null>(null);

	useEffect(() => setTitleDraft(cover.title), [cover.title]);

	const update = (change: Partial<CoverSettings>) => {
		const next = { ...cover, ...change };
		if (JSON.stringify(next) === JSON.stringify(project.settings.cover)) return;
		void editor.project.updateSettings({ settings: { cover: next } });
	};

	// The preview follows the cover, the canvas and the timeline.
	const previewKey = JSON.stringify([cover, project.settings.canvasSize, project.settings.background]);
	useEffect(() => {
		let cancelled = false;
		const timer = setTimeout(async () => {
			try {
				const { blob } = await renderCoverPng({ editor, cover });
				if (cancelled) return;
				const url = URL.createObjectURL(blob);
				if (previewUrl.current) URL.revokeObjectURL(previewUrl.current);
				previewUrl.current = url;
				setPreview(url);
			} catch {
				// An empty timeline has nothing to show.
			}
		}, 250);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
		// biome-ignore lint/correctness/useExhaustiveDependencies: the key covers the cover and settings; tracks change the frame
	}, [previewKey, tracks, editor]);
	useEffect(() => () => {
		if (previewUrl.current) URL.revokeObjectURL(previewUrl.current);
	}, []);

	const save = async (shape: CoverShape) => {
		setBusy(true);
		try {
			const { blob, width, height } = await renderCoverPng({ editor, cover, shape });
			const url = URL.createObjectURL(blob);
			const link = document.createElement("a");
			link.href = url;
			link.download = fileName(project.metadata.name, width, height);
			link.click();
			setTimeout(() => URL.revokeObjectURL(url), 10_000);
			setStatus(t("Cover saved ({width}×{height}).", { width, height }));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(false);
		}
	};

	return (
		<div className="flex flex-col gap-3 px-2 pb-2" data-testid="cover-settings">
			<p className="text-muted-foreground text-xs">
				{t("A frame and a title, saved as a picture and embedded as the MP4's cover when exporting.")}
			</p>
			{preview && (
				<img
					src={preview}
					alt={t("Cover")}
					className="max-h-48 w-full rounded-sm border object-contain"
					data-testid="cover-preview"
				/>
			)}
			<label className="flex flex-col gap-1 text-xs">
				<span className="text-muted-foreground">{t("Cover title")}</span>
				<Input
					value={titleDraft}
					placeholder={t("Title on the cover")}
					data-testid="cover-title"
					onChange={(event) => setTitleDraft(event.target.value)}
					onBlur={() => update({ title: titleDraft.trim() })}
					onKeyDown={(event) => {
						if (event.key === "Enter") update({ title: titleDraft.trim() });
					}}
				/>
			</label>
			<div className="flex flex-wrap items-center gap-1">
				<span className="text-muted-foreground mr-1 text-xs">{t("Style")}</span>
				{COVER_STYLES.map((style) => (
					<Button
						key={style}
						size="sm"
						variant={cover.style === style ? "secondary" : "outline"}
						data-testid={`cover-style-${style}`}
						onClick={() => update({ style })}
					>
						{t(STYLE_LABELS[style])}
					</Button>
				))}
			</div>
			<div className="flex flex-wrap items-center gap-1">
				<span className="text-muted-foreground mr-1 text-xs">
					{t("Frame")} {clock(cover.time)}
				</span>
				<Button
					size="sm"
					variant="outline"
					data-testid="cover-playhead"
					onClick={() => update({ time: editor.playback.getCurrentTime() / TICKS_PER_SECOND })}
				>
					{t("Use playhead")}
				</Button>
				<Button
					size="sm"
					variant="outline"
					disabled={busy}
					data-testid="cover-suggest"
					onClick={async () => {
						setBusy(true);
						setStatus(t("Looking for a good cover frame…"));
						try {
							const time = await suggestCoverTime({ editor });
							update({ time });
							editor.playback.seek({ time: mediaTimeFromSeconds({ seconds: time }) });
							setStatus(t("Cover frame picked; the playhead is on it."));
						} catch (error) {
							setStatus(error instanceof Error ? error.message : String(error));
						} finally {
							setBusy(false);
						}
					}}
				>
					{t("Suggest a frame")}
				</Button>
			</div>
			<label className="flex items-center gap-2 text-xs">
				<Checkbox checked={cover.embed} data-testid="cover-embed" onCheckedChange={(checked) => update({ embed: checked === true })} />
				{t("Embed as the cover of MP4 exports")}
			</label>
			<div className="flex flex-wrap items-center gap-1">
				<span className="text-muted-foreground mr-1 text-xs">{t("Save PNG")}</span>
				{COVER_SHAPES.map((shape) => {
					const size = coverSize(shape, project.settings.canvasSize);
					return (
						<Button
							key={shape}
							size="sm"
							variant="outline"
							disabled={busy}
							title={`${size.width}×${size.height}`}
							data-testid={`cover-save-${shape}`}
							onClick={() => save(shape)}
						>
							{t(SHAPE_LABELS[shape])}
						</Button>
					);
				})}
			</div>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

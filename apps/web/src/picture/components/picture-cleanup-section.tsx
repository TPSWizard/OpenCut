"use client";

import { useState } from "react";
import { toast } from "sonner";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { ImageElement } from "@/timeline/types";
import { cn } from "@/utils/ui";
import { PICTURE_SOURCE_PARAM, cleanUpPicture, restoreOriginalPicture, storedPath } from "../apply";
import { DEFAULT_PICTURE_CLEANUP } from "../cleanup";

const TINTS: Array<{ id: string; value: string | null }> = [
	{ id: "none", value: null },
	{ id: "white", value: "#ffffff" },
	{ id: "black", value: "#111111" },
];

/** Deterministic cleanup of a drawn picture: white paper out, halo out, feather, tint, crop, path. */
export function PictureCleanupSection({ element, trackId }: { element: ImageElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [removeWhite, setRemoveWhite] = useState(DEFAULT_PICTURE_CLEANUP.removeWhite);
	const [decontaminate, setDecontaminate] = useState(DEFAULT_PICTURE_CLEANUP.decontaminate);
	const [feather, setFeather] = useState(DEFAULT_PICTURE_CLEANUP.feather);
	const [tint, setTint] = useState<string | null>(null);
	const [crop, setCrop] = useState(DEFAULT_PICTURE_CLEANUP.crop);
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const cleaned = Boolean(element.params[PICTURE_SOURCE_PARAM]);
	const path = storedPath({ element });

	const run = async () => {
		setBusy(true);
		setStatus(null);
		try {
			const result = await cleanUpPicture({
				editor,
				trackId,
				elementId: element.id,
				options: { removeWhite, decontaminate, feather, tint, crop, trace: true },
			});
			setStatus(
				result.path
					? t("Cleaned: {width}×{height}, a {count}-point path traced.", {
							width: result.width,
							height: result.height,
							count: result.path.points.length,
						})
					: t("Cleaned: {width}×{height}; no path could be traced.", { width: result.width, height: result.height }),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Cleanup failed"));
		} finally {
			setBusy(false);
		}
	};

	const toggle = (label: string, value: boolean, set: (next: boolean) => void) => (
		<label className="flex items-center justify-between gap-2 text-xs">
			<span>{label}</span>
			<Switch checked={value} onCheckedChange={set} />
		</label>
	);

	return (
		<Section sectionKey={`${element.id}:picture-cleanup`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>{t("Clean up picture")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="picture-cleanup">
					<p className="text-muted-foreground text-xs">
						{t("For drawings on white: takes the paper out, then tidies the edge. Nothing leaves this device.")}
					</p>
					{toggle(t("Remove white background"), removeWhite, setRemoveWhite)}
					{toggle(t("Remove white halo"), decontaminate, setDecontaminate)}
					{toggle(t("Crop to the drawing"), crop, setCrop)}
					<div className="flex flex-wrap items-center gap-1 text-xs">
						<span className="text-muted-foreground mr-1">{t("Feather")}</span>
						{[0, 1, 2, 4].map((value) => (
							<button
								key={value}
								type="button"
								className={cn("rounded-md border px-2 py-0.5", feather === value ? "bg-accent border-foreground/30" : "text-muted-foreground")}
								onClick={() => setFeather(value)}
							>
								{value === 0 ? t("Off") : `${value} px`}
							</button>
						))}
					</div>
					<div className="flex flex-wrap items-center gap-1 text-xs">
						<span className="text-muted-foreground mr-1">{t("Ink")}</span>
						{TINTS.map((option) => (
							<button
								key={option.id}
								type="button"
								className={cn("rounded-md border px-2 py-0.5", tint === option.value ? "bg-accent border-foreground/30" : "text-muted-foreground")}
								onClick={() => setTint(option.value)}
							>
								{option.id === "none" ? t("As drawn") : option.id === "white" ? t("White") : t("Black")}
							</button>
						))}
						<input
							type="color"
							aria-label={t("Ink colour")}
							value={tint ?? "#ffffff"}
							onChange={(event) => setTint(event.target.value)}
							className="h-6 w-8 cursor-pointer rounded border"
						/>
					</div>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" onClick={run} disabled={busy} data-testid="picture-cleanup-run">
							{busy ? t("Cleaning...") : cleaned ? t("Clean again") : t("Clean up")}
						</Button>
						{cleaned ? (
							<Button
								size="sm"
								variant="outline"
								disabled={busy}
								onClick={() => {
									if (restoreOriginalPicture({ editor, trackId, elementId: element.id })) {
										setStatus(t("The original picture is back."));
									}
								}}
							>
								{t("Restore original")}
							</Button>
						) : null}
						{path ? (
							<Button
								size="sm"
								variant="ghost"
								onClick={() => {
									void navigator.clipboard.writeText(JSON.stringify(path)).then(
										() => toast.success(t("Path copied")),
										() => toast.error(t("The clipboard is not available here")),
									);
								}}
							>
								{t("Copy path ({count} points)", { count: path.points.length })}
							</Button>
						) : null}
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="picture-cleanup-status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

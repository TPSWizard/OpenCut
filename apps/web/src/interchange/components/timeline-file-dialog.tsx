"use client";

import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { downloadBlob } from "@/utils/browser";
import { cn } from "@/utils/ui";
import { exportProjectTimeline, importProjectTimeline } from "../project";
import type { TimelineFormat } from "../timeline";

const FORMATS: { id: TimelineFormat; label: string }[] = [
	{ id: "otio", label: "OpenTimelineIO" },
	{ id: "fcpxml", label: "FCPXML" },
	{ id: "edl", label: "EDL" },
];

/**
 * Timeline files (row 111): download the timeline as OpenTimelineIO, FCPXML
 * or a CMX 3600 EDL for other editors, or bring one in from them.
 */
export function TimelineFileDialog({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const [format, setFormat] = useState<TimelineFormat>("otio");
	const [status, setStatus] = useState<string | null>(null);
	const input = useRef<HTMLInputElement>(null);

	const download = () => {
		try {
			const { fileName, mimeType, text, notes } = exportProjectTimeline({ editor, format });
			downloadBlob({ blob: new Blob([text], { type: mimeType }), filename: fileName });
			setStatus(
				notes.length > 0
					? t("Saved {file}. Not carried: {notes}", { file: fileName, notes: notes.map((note) => t(note)).join(" ") })
					: t("Saved {file}.", { file: fileName }),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : String(error));
		}
	};

	const bringIn = async (file: File) => {
		try {
			const result = importProjectTimeline({ editor, text: await file.text(), fileName: file.name });
			const parts: string[] = [];
			if (result.clips > 0) {
				parts.push(t("Imported {clips} clip(s) on {tracks} track(s).", { clips: result.clips, tracks: result.tracks }));
			} else if (result.missing.length === 0) {
				parts.push(t("The timeline has no video or audio clips to bring in."));
			}
			if (result.missing.length > 0) {
				parts.push(t("Import these files into the project first, then the timeline again: {files}", { files: result.missing.join(", ") }));
			}
			if (result.notes.length > 0) {
				parts.push(t("Not carried: {notes}", { notes: result.notes.map((note) => t(note)).join(" ") }));
			}
			setStatus(parts.join(" "));
		} catch (error) {
			setStatus(t("Timeline not read: {error}", { error: error instanceof Error ? error.message : String(error) }));
		}
	};

	return (
		<Dialog
			open={isOpen}
			onOpenChange={(open) => {
				if (!open) setStatus(null);
				onOpenChange(open);
			}}
		>
			<DialogContent data-testid="timeline-file-dialog">
				<DialogHeader>
					<DialogTitle>{t("Timeline file")}</DialogTitle>
					<DialogDescription>
						{t(
							"Move the edit to or from another editor (DaVinci Resolve, Premiere, Final Cut Pro): clips, trims, speed changes and dissolves. The media files themselves are not included.",
						)}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="gap-3">
					<Label>{t("Format")}</Label>
					<div className="flex gap-1">
						{FORMATS.map(({ id, label }) => (
							<Button
								key={id}
								size="sm"
								variant="outline"
								className={cn("flex-1", format === id && "border-primary text-primary")}
								onClick={() => setFormat(id)}
								data-testid={`timeline-format-${id}`}
							>
								{label}
							</Button>
						))}
					</div>
					<p className="text-muted-foreground text-xs">
						{format === "edl"
							? t("An EDL holds the main video track and two sound channels.")
							: format === "fcpxml"
								? t("FCPXML 1.10: the main track is the storyline; the other tracks are connected clips.")
								: t("OpenTimelineIO: every video and audio track.")}
					</p>
					<input
						ref={input}
						type="file"
						accept=".otio,.fcpxml,.edl,.xml,application/json,application/xml,text/plain"
						className="hidden"
						data-testid="timeline-file-input"
						onChange={(event) => {
							const file = event.target.files?.[0];
							event.target.value = "";
							if (file) void bringIn(file);
						}}
					/>
					{status && (
						<p className="text-muted-foreground text-xs" role="status" data-testid="timeline-file-status">
							{status}
						</p>
					)}
				</DialogBody>
				<DialogFooter>
					<Button variant="outline" onClick={() => input.current?.click()} data-testid="timeline-file-import">
						{t("Import a timeline…")}
					</Button>
					<Button onClick={download} data-testid="timeline-file-export">
						{t("Download")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TimelineElement, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { cutCount, multicamGroup, multicamSwitchCommand } from "../group";
import { useMulticamGridStore } from "../grid";
import { type ElementRef, syncMulticam } from "../sync";

/** With two or more video clips selected: line them up by their sound as multicam angles. */
export function MulticamSyncPanel({ targets }: { targets: readonly ElementRef[] }) {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const [status, setStatus] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	if (targets.length < 2) return null;
	const all = [...tracks.overlay, tracks.main, ...tracks.audio];
	const allVideos = targets.every(
		(ref) =>
			(all.find((track) => track.id === ref.trackId)?.elements as TimelineElement[] | undefined)?.find((element) => element.id === ref.elementId)
				?.type === "video",
	);
	if (!allVideos) return null;

	return (
		<div className="flex w-full max-w-64 flex-col gap-2 px-4" data-testid="multicam-sync-panel">
			<p className="text-xs font-medium">{t("Multicam")}</p>
			<p className="text-muted-foreground text-xs">
				{t(
					"Lines the selected camera angles up by their sound (the first one you selected stays put) and groups them, so you can switch between them while playing. Each angle must be on its own track.",
				)}
			</p>
			<Button
				size="sm"
				variant="outline"
				disabled={busy}
				data-testid="multicam-sync"
				onClick={async () => {
					setBusy(true);
					setStatus(t("Listening to each angle to line them up…"));
					try {
						const { lags } = await syncMulticam({ editor, elements: [...targets] });
						setStatus(t("{count} angles lined up by their sound; only the first one's sound plays.", { count: lags.length }));
					} catch (error) {
						setStatus(error instanceof Error ? error.message : String(error));
					} finally {
						setBusy(false);
					}
				}}
			>
				{t("Sync by audio")}
			</Button>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

/** On a clip in a multicam group: switch the group to an angle from the playhead on, also while playing. */
export function MulticamSection({ element, trackId }: { element: VideoElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveScene().tracks);
	const [status, setStatus] = useState<string | null>(null);
	const gridOpen = useMulticamGridStore((state) => state.elementId !== null);
	const group = multicamGroup(tracks, element.id);
	if (!group) return null;
	const cuts = cutCount(group.angles);

	const show = (angle: number | null) => {
		try {
			editor.command.execute({
				command: multicamSwitchCommand({
					tracks: editor.scenes.getActiveScene().tracks,
					elementId: element.id,
					time: editor.playback.getCurrentTime() / TICKS_PER_SECOND,
					angle,
				}),
			});
			setStatus(null);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : String(error));
		}
	};

	return (
		<Section sectionKey={`${element.id}:multicam`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>{t("Multicam")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="multicam-section" data-track={trackId}>
					<p className="text-muted-foreground text-xs">
						{t("Click an angle to show it from the playhead on; it works while playing. {cuts} cuts so far.", { cuts })}
					</p>
					<div className="flex flex-wrap gap-1">
						{group.angles.map((angle) => (
							<Button
								key={angle.element.id}
								size="sm"
								variant="outline"
								className={angle.element.id === element.id ? "border-primary text-primary" : undefined}
								onClick={() => show(angle.angle)}
								data-testid={`multicam-angle-${angle.angle + 1}`}
								title={angle.element.name}
							>
								{angle.angle + 1} {angle.element.name}
							</Button>
						))}
						<Button size="sm" variant="ghost" disabled={cuts === 0} onClick={() => show(null)} data-testid="multicam-clear">
							{t("Clear cuts")}
						</Button>
					</div>
					<Button
						size="sm"
						variant={gridOpen ? "secondary" : "outline"}
						data-testid="multicam-grid-panel-toggle"
						onClick={() => useMulticamGridStore.getState().setElementId(gridOpen ? null : element.id)}
					>
						{gridOpen ? t("Hide the grid") : t("Show the grid")}
					</Button>
					<p className="text-muted-foreground text-xs">
						{t("The grid shows every angle in the preview; press 1-9 to cut to that angle at the playhead, also while playing.")}
					</p>
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

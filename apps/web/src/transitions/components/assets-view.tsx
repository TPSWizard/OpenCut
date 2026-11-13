"use client";

import { useMemo, useState } from "react";
import { toast } from "sonner";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import {
	DEFAULT_TRANSITION_DURATION,
	getTransitionCatalogue,
	searchTransitions,
	type TransitionInfo,
} from "@/transitions";
import { getClipTransition, getSelectedVisualClips, setClipTransition, type TransitionSide } from "@/transitions/apply";
import { cn } from "@/utils/ui";

type ApplyTo = TransitionSide | "both";

/**
 * The transitions catalogue. Clicking an entry gives every selected visual
 * clip that transition at its start, its end or both (keeping a duration
 * one already has there).
 */
export function TransitionsView() {
	const t = useT();
	const [query, setQuery] = useState("");
	const [applyTo, setApplyTo] = useState<ApplyTo>("end");
	const editor = useEditor();
	// A primitive key, so a fresh snapshot object cannot re-render us forever.
	useEditor((e) =>
		e.selection
			.getSelectedElements()
			.map((ref) => ref.elementId)
			.join(","),
	);
	const selected = getSelectedVisualClips({ editor });
	const sides: TransitionSide[] = applyTo === "both" ? ["start", "end"] : [applyTo];
	const activeTypes = new Set(
		selected
			.flatMap(({ track, element }) => sides.map((side) => getClipTransition({ track, element, side })?.type))
			.filter((type): type is string => Boolean(type)),
	);
	const results = useMemo(() => searchTransitions({ query }), [query]);
	const total = getTransitionCatalogue().length;

	const apply = (info: TransitionInfo) => {
		const clips = getSelectedVisualClips({ editor });
		if (clips.length === 0) {
			toast.info(t("Select a clip first, then pick where the transition goes: its start, its end or both."));
			return;
		}
		for (const side of sides) {
			for (const { element } of clips) {
				// Re-read the track each time: an earlier change may have touched it.
				const tracks = editor.scenes.getActiveScene().tracks;
				const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) =>
					candidate.elements.some((item) => item.id === element.id),
				);
				const current = track ? track.elements.find((item) => item.id === element.id) : undefined;
				if (!track || !current) continue;
				const existing = getClipTransition({ track, element: current, side });
				setClipTransition({
					editor,
					track,
					element: current,
					side,
					transition: { type: info.name, duration: existing?.duration ?? DEFAULT_TRANSITION_DURATION },
				});
			}
		}
	};

	return (
		<PanelView
			title={t("Transitions")}
			actions={
				<Input
					value={query}
					onChange={(event) => setQuery(event.target.value)}
					placeholder={t("Search {count} transitions", { count: total })}
					className="h-8 w-44 text-xs"
					aria-label={t("Search transitions")}
				/>
			}
		>
			<div className="mb-2 flex items-center gap-1" role="radiogroup" aria-label={t("Apply to")}>
				<span className="text-muted-foreground mr-1 text-xs">{t("Apply to")}</span>
				{(["start", "end", "both"] as const).map((side) => (
					<button
						key={side}
						type="button"
						role="radio"
						aria-checked={applyTo === side}
						data-testid={`transition-apply-${side}`}
						className={cn(
							"rounded-md border px-2 py-0.5 text-xs",
							applyTo === side ? "bg-accent border-foreground/30" : "text-muted-foreground",
						)}
						onClick={() => setApplyTo(side)}
					>
						{side === "start" ? t("Start") : side === "end" ? t("End") : t("Both")}
					</button>
				))}
			</div>
			<p className="text-muted-foreground mb-3 text-xs">
				{selected.length === 0
					? t("Select a clip, then pick a transition for its start, its end or both.")
					: selected.length === 1
						? t("Applies to 1 selected clip.")
						: t("Applies to {count} selected clips.", { count: selected.length })}
			</p>
			<div
				className="grid gap-2"
				style={{ gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))" }}
			>
				{results.map((info) => (
					<button
						type="button"
						key={info.name}
						onClick={() => apply(info)}
						title={info.description ?? info.name}
						className={cn(
							"bg-muted/40 hover:bg-muted flex flex-col items-start gap-1 rounded-md border px-2.5 py-2 text-left transition-colors",
							activeTypes.has(info.name) && "border-primary bg-primary/10",
						)}
					>
						<span className="text-xs font-medium">{spaced(info.name)}</span>
						<span className="text-muted-foreground line-clamp-2 text-[10px] leading-tight">
							{info.description ?? info.tags.join(", ")}
						</span>
					</button>
				))}
				{results.length === 0 && (
					<p className="text-muted-foreground col-span-full text-xs">
						{t("Nothing matches \"{query}\".", { query })}
					</p>
				)}
			</div>
		</PanelView>
	);
}

/** `DirectionalWipe` as `Directional wipe`. */
export function spaced(name: string): string {
	const words = name.replace(/([a-z0-9])([A-Z])/g, "$1 $2");
	return words.charAt(0) + words.slice(1).toLowerCase();
}

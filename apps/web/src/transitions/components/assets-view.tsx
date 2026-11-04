"use client";

import { useMemo, useState } from "react";
import { toast } from "sonner";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import {
	DEFAULT_TRANSITION_DURATION,
	getTransitionCatalogue,
	searchTransitions,
	type TransitionInfo,
} from "@/transitions";
import { getSelectedVisualClips, setElementTransition } from "@/transitions/apply";
import { cn } from "@/utils/ui";

/**
 * The transitions catalogue. Clicking an entry gives every selected visual
 * clip that transition out of its end (keeping a duration it already has).
 */
export function TransitionsView() {
	const [query, setQuery] = useState("");
	const editor = useEditor();
	useEditor((e) => e.selection.getSnapshot());
	const selected = getSelectedVisualClips({ editor });
	const activeTypes = new Set(
		selected
			.map(({ element }) => ("transition" in element ? element.transition?.type : undefined))
			.filter((type): type is string => Boolean(type)),
	);
	const results = useMemo(() => searchTransitions({ query }), [query]);
	const total = getTransitionCatalogue().length;

	const apply = (info: TransitionInfo) => {
		const clips = getSelectedVisualClips({ editor });
		if (clips.length === 0) {
			toast.info("Select a clip first; the transition plays out of its end.");
			return;
		}
		for (const { track, element } of clips) {
			const current = "transition" in element ? element.transition : undefined;
			setElementTransition({
				editor,
				trackId: track.id,
				elementId: element.id,
				transition: {
					type: info.name,
					duration: current?.duration ?? DEFAULT_TRANSITION_DURATION,
				},
			});
		}
	};

	return (
		<PanelView
			title="Transitions"
			actions={
				<Input
					value={query}
					onChange={(event) => setQuery(event.target.value)}
					placeholder={`Search ${total} transitions`}
					className="h-8 w-44 text-xs"
					aria-label="Search transitions"
				/>
			}
		>
			<p className="text-muted-foreground mb-3 text-xs">
				{selected.length === 0
					? "Select a clip, then pick a transition to play out of its end into the next clip."
					: `Applies to ${selected.length} selected clip${selected.length === 1 ? "" : "s"}.`}
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
						Nothing matches "{query}".
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

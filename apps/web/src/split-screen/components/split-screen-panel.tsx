"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { applySplitScreen, type SplitTarget } from "../apply";
import { layoutsFor, type SplitLayout } from "../layout";

/** A small picture of a layout's cells. */
function LayoutIcon({ layout }: { layout: SplitLayout }) {
	return (
		<svg viewBox="0 0 32 18" className="h-5 w-9" aria-hidden="true">
			{layout.cells.map(([x, y, w, h], index) => (
				<rect
					// biome-ignore lint/suspicious/noArrayIndexKey: cells have no ids
					key={index}
					x={x * 32 + 0.75}
					y={y * 18 + 0.75}
					width={w * 32 - 1.5}
					height={h * 18 - 1.5}
					rx={1}
					className="fill-muted-foreground/30 stroke-muted-foreground"
					strokeWidth={0.75}
				/>
			))}
		</svg>
	);
}

/**
 * Split screen for 2 to 4 selected video or picture clips: one click puts
 * them side by side, stacked or in a grid, in the order they were selected.
 */
export function SplitScreenPanel({ targets }: { targets: readonly SplitTarget[] }) {
	const t = useT();
	const editor = useEditor();
	const [status, setStatus] = useState<string | null>(null);
	const layouts = layoutsFor(targets.length);
	if (layouts.length === 0) return null;
	const label = (layout: SplitLayout) => t(layout.label);

	return (
		<div className="flex w-full max-w-64 flex-col gap-2 px-4" data-testid="split-screen-panel">
			<p className="text-xs font-medium">{t("Split screen")}</p>
			<p className="text-muted-foreground text-xs">
				{t("Puts the selected video and picture clips side by side or in a grid, in the order you selected them, each filling its part of the canvas.")}
			</p>
			<div className="grid grid-cols-1 gap-1">
				{layouts.map((layout) => (
					<Button
						key={layout.id}
						size="sm"
						variant="outline"
						className="justify-start gap-2"
						data-testid={`split-${layout.id}`}
						onClick={() => {
							try {
								applySplitScreen({ editor, layoutId: layout.id, targets });
								setStatus(t("{count} clips arranged.", { count: targets.length }));
							} catch (error) {
								setStatus(error instanceof Error ? error.message : String(error));
							}
						}}
					>
						<LayoutIcon layout={layout} />
						{label(layout)}
					</Button>
				))}
			</div>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

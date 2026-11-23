"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { type ColourMatchRef, matchColours } from "../apply";

/**
 * Colour match for 2 or more selected video or picture clips: the others
 * take on the colours of the first one selected, at the playhead.
 */
export function ColourMatchPanel({ targets }: { targets: readonly ColourMatchRef[] }) {
	const t = useT();
	const editor = useEditor();
	const [status, setStatus] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	if (targets.length < 2) return null;
	const [reference, ...rest] = targets;

	return (
		<div className="flex w-full max-w-64 flex-col gap-2 px-4" data-testid="colour-match-panel">
			<p className="text-xs font-medium">{t("Colour match")}</p>
			<p className="text-muted-foreground text-xs">
				{t("Makes the other selected clips take on the colours of the first one you selected, at the playhead (or its middle). Each gets a LUT effect you can turn down or remove.")}
			</p>
			<Button
				size="sm"
				variant="outline"
				disabled={busy}
				data-testid="colour-match"
				onClick={async () => {
					setBusy(true);
					setStatus(t("Matching colours…"));
					try {
						const result = await matchColours({ editor, reference, targets: rest });
						setStatus(result.targets.length === 1 ? t("1 clip matched.") : t("{count} clips matched.", { count: result.targets.length }));
					} catch (error) {
						setStatus(error instanceof Error ? error.message : String(error));
					} finally {
						setBusy(false);
					}
				}}
			>
				{t("Match to the first selected")}
			</Button>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

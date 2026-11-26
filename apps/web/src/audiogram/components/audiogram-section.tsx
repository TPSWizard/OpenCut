"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TimelineElement } from "@/timeline";
import { makeAudiogram } from "../audiogram";

/**
 * Audiogram (row 100) on a video or audio clip: listens to its sound and
 * adds a layer of bars that move with it over the clip's span.
 */
export function AudiogramSection({ element, trackId }: { element: TimelineElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);

	const make = async () => {
		setBusy(true);
		setStatus(t("Listening to the clip…"));
		try {
			await makeAudiogram({ editor, trackId, elementId: element.id });
			setStatus(t("Audiogram added over the clip. Pick its style in its Graphic tab."));
		} catch (error) {
			setStatus(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(false);
		}
	};

	return (
		<Section sectionKey={`${element.id}:audiogram`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>{t("Audiogram")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-2" data-testid="audiogram-section">
					<p className="text-muted-foreground text-xs">
						{t("Listens to this clip and adds a layer over its span whose bars move with the sound (up to 10 minutes). Style it once it is added.")}
					</p>
					<Button size="sm" onClick={make} disabled={busy} data-testid="audiogram-make">
						{t("Make audiogram")}
					</Button>
					{status && <p className="text-muted-foreground text-xs" data-testid="audiogram-status">{status}</p>}
				</div>
			</SectionContent>
		</Section>
	);
}

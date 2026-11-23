"use client";

import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { AudioCapableElement } from "@/timeline/audio-state";
import type { TimelineElement } from "@/timeline/types";
import { AUDIO_FX_PARAMS, audioFxOf, audioFxPresetList, fxParams, matchingPreset } from "../fx";

/**
 * Audio effects on a clip: presets, then the equaliser, compressor and
 * limiter settings. Heard as the clip plays and in exports; the clip's
 * media is not changed.
 */
export function AudioFxSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const fx = audioFxOf(element.params as Record<string, unknown>);
	const current = matchingPreset(fx);

	return (
		<Section sectionKey={`${element.id}:audio-fx`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>{t("Audio effects")}</SectionTitle>
			</SectionHeader>
			<SectionContent className="flex flex-col gap-2" data-testid="audio-fx-section">
				<p className="text-muted-foreground text-xs">
					{t("Equaliser, compressor and limiter on this clip, heard as it plays and in exports; the original stays as it is.")}
				</p>
				<div className="flex flex-wrap gap-1">
					{audioFxPresetList().map((preset) => (
						<Button
							key={preset.id}
							size="sm"
							variant={current === preset.id ? "secondary" : "outline"}
							data-testid={`audio-fx-preset-${preset.id}`}
							onClick={() =>
								editor.command.execute({
									command: new UpdateElementsCommand({
										updates: [
											{
												trackId,
												elementId: element.id,
												patch: { params: { ...element.params, ...fxParams(preset.fx) } } as Partial<TimelineElement>,
											},
										],
									}),
								})
							}
						>
							{t(preset.label)}
						</Button>
					))}
				</div>
			</SectionContent>
			<ElementParamsTab element={element} trackId={trackId} paramKeys={AUDIO_FX_PARAMS} sectionKey="audio-fx-params" />
		</Section>
	);
}

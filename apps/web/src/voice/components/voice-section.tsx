"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import type { AudioCapableElement } from "@/timeline/audio-state";
import { VOICE_EFFECTS, applyVoiceEffectToElement, type VoiceEffectId } from "../apply";

/**
 * Voice effects for a clip: a changed copy of its audio (deeper, higher,
 * robot, echo, radio, hall…) as a new media asset the clip then plays.
 */
export function VoiceSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const [busy, setBusy] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const supported = element.type === "video" || (element.type === "audio" && element.sourceType === "upload");

	const run = async (effect: VoiceEffectId) => {
		setBusy("Decoding…");
		setStatus(null);
		try {
			const result = await applyVoiceEffectToElement({
				editor,
				trackId,
				elementId: element.id,
				effect,
				onProgress: (step) => setBusy(step === "decoding" ? "Decoding…" : step === "processing" ? "Changing the voice…" : "Saving…"),
			});
			setStatus(
				`Saved ${result.assetName} (${result.seconds.toFixed(1)} s); the clip now plays it.` +
					(element.type === "video" ? " The audio was separated onto its own track." : ""),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "The voice effect failed");
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey={`${element.id}:voice`} collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>Voice effects</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="voice-section">
					<p className="text-muted-foreground text-xs">
						Writes a changed copy of the clip's audio, the same length as the original, and the clip plays it. Undo puts
						the original back.
					</p>
					<div className="grid grid-cols-2 gap-2">
						{VOICE_EFFECTS.map((effect) => (
							<Button key={effect.id} size="sm" variant="outline" disabled={!supported || busy !== null} onClick={() => run(effect.id)}>
								{effect.label}
							</Button>
						))}
					</div>
					{busy && <p className="text-muted-foreground text-xs">{busy}</p>}
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

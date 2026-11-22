"use client";

import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { generateCaptions, insertGeneratedCaptions } from "@/captions/generate";
import { useEditor } from "@/editor/use-editor";
import { isRippleScope, type RippleScope } from "@/silence/detect";
import type { AudioCapableElement } from "@/timeline/audio-state";
import { TICKS_PER_SECOND, roundMediaTime } from "@/wasm";
import { cn } from "@/utils/ui";
import { transcriptCuts } from "../cuts";
import { clipTranscript, cutTranscriptWords, pieceAfter } from "../edit";

const RIPPLE_LABELS: Record<RippleScope, string> = {
	all: "Close gaps on every track",
	track: "Close gaps on this track",
	none: "Leave gaps",
};

/**
 * Text-based editing for a clip: its transcript (the word-timed captions
 * over it) as text; struck words are cut out of the clip, with the
 * captions retimed to match.
 */
export function TranscriptSection({ element, trackId }: { element: AudioCapableElement; trackId: string }) {
	const editor = useEditor();
	const tracks = useEditor((current) => current.scenes.getActiveScene().tracks);
	const transcript = useMemo(() => clipTranscript({ tracks, trackId, elementId: element.id }), [tracks, trackId, element.id]);
	const words = transcript?.words ?? [];
	const signature = useMemo(() => words.map((word) => `${word.start}:${word.text}`).join("|"), [words]);
	const [struck, setStruck] = useState<Set<number>>(() => new Set());
	const anchor = useRef<number | null>(null);
	const [ripple, setRipple] = useState<RippleScope>("all");
	const [busy, setBusy] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);

	// The words changed under us (an edit, an undo): the indices no longer hold.
	// biome-ignore lint/correctness/useExhaustiveDependencies: reset on a new transcript only
	useEffect(() => {
		setStruck(new Set());
		anchor.current = null;
	}, [signature]);

	const cuts = useMemo(
		() => (transcript ? transcriptCuts({ words, deleted: struck, from: transcript.from, to: transcript.to }) : []),
		[transcript, words, struck],
	);
	const cutSeconds = cuts.reduce((sum, cut) => sum + (cut.end - cut.start), 0);

	const onWord = (index: number, event: MouseEvent) => {
		if (event.altKey) {
			editor.playback.seek({ time: roundMediaTime({ time: words[index].start * TICKS_PER_SECOND }) });
			return;
		}
		setStruck((previous) => {
			const next = new Set(previous);
			if (event.shiftKey && anchor.current !== null && anchor.current !== index) {
				const strike = previous.has(anchor.current);
				for (let word = Math.min(anchor.current, index); word <= Math.max(anchor.current, index); word++) {
					if (strike) next.add(word);
					else next.delete(word);
				}
			} else if (!next.delete(index)) {
				next.add(index);
			}
			return next;
		});
		anchor.current = index;
	};

	const transcribe = async () => {
		setBusy("Transcribing…");
		setStatus(null);
		try {
			const { captions } = await generateCaptions({ editor });
			if (insertGeneratedCaptions({ editor, captions }) === null) setStatus("No speech was found.");
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Transcription failed");
		} finally {
			setBusy(null);
		}
	};

	const cut = () => {
		try {
			const result = cutTranscriptWords({ editor, trackId, elementId: element.id, deleted: struck, ripple });
			// The clip is in pieces now; keep the one after the first cut selected so editing goes on.
			const next = pieceAfter({ tracks: editor.scenes.getActiveScene().tracks, pieces: result.pieces, time: result.cuts[0]?.start ?? 0 });
			if (next) editor.selection.setSelectedElements({ elements: [next] });
			setStatus(
				`${result.words} word${result.words === 1 ? "" : "s"} cut in ${result.cuts.length} place${result.cuts.length === 1 ? "" : "s"} (${result.seconds.toFixed(2)} s); the captions follow.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not cut");
		}
	};

	return (
		<Section sectionKey={`${element.id}:transcript`} collapsible defaultOpen>
			<SectionHeader>
				<SectionTitle>Transcript</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="transcript-section">
					<p className="text-muted-foreground text-xs">
						Click words to strike them (Shift-click strikes a run, Alt-click jumps there), then cut. Uses the captions
						over this clip; they are retimed with the cut.
					</p>
					{words.length === 0 ? (
						<div className="flex flex-col gap-2">
							<p className="text-muted-foreground text-xs">No captions over this clip yet.</p>
							<Button size="sm" variant="outline" onClick={transcribe} disabled={busy !== null}>
								{busy ?? "Transcribe to captions"}
							</Button>
						</div>
					) : (
						<>
							<div
								className="bg-muted/40 max-h-72 overflow-y-auto rounded-md p-2 text-sm leading-7"
								data-testid="transcript-words"
							>
								{words.map((word, index) => (
									<button
										// biome-ignore lint/suspicious/noArrayIndexKey: words are addressed by index
										key={index}
										type="button"
										aria-pressed={struck.has(index)}
										title={`${word.start.toFixed(2)} s`}
										onClick={(event) => onWord(index, event)}
										className={cn(
											"hover:bg-accent mr-1 rounded px-0.5",
											struck.has(index) && "bg-destructive/15 text-destructive line-through",
										)}
									>
										{word.text.trim()}
									</button>
								))}
							</div>
							<div className="flex flex-col gap-1.5">
								<Label htmlFor="transcript-ripple">After cutting</Label>
								<Select value={ripple} onValueChange={(value) => isRippleScope(value) && setRipple(value)}>
									<SelectTrigger id="transcript-ripple" className="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										{(Object.keys(RIPPLE_LABELS) as RippleScope[]).map((scope) => (
											<SelectItem key={scope} value={scope}>
												{RIPPLE_LABELS[scope]}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
							</div>
							<div className="flex gap-2">
								<Button size="sm" onClick={cut} disabled={struck.size === 0}>
									{struck.size === 0 ? "Cut struck words" : `Cut ${struck.size} word${struck.size === 1 ? "" : "s"} (${cutSeconds.toFixed(1)} s)`}
								</Button>
								<Button size="sm" variant="outline" onClick={() => setStruck(new Set())} disabled={struck.size === 0}>
									Clear
								</Button>
							</div>
						</>
					)}
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

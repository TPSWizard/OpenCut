"use client";

import { useState } from "react";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { DEFAULT_KOKORO_VOICE, KOKORO_VOICES, generateVoiceover } from "../generate";
import { splitScript } from "../script";

const SPEEDS = [0.8, 0.9, 1, 1.1, 1.25];

/** "af_heart" → "Heart (American, female)". */
function voiceLabel({ id, t }: { id: string; t: ReturnType<typeof useT> }): string {
	const [code, name] = id.split("_");
	const accent = code[0] === "a" ? t("American") : t("British");
	const gender = code[1] === "f" ? t("female") : t("male");
	return `${name.charAt(0).toUpperCase()}${name.slice(1)} (${accent}, ${gender})`;
}

/** Voice-over from a script, spoken on this device by Kokoro. */
export function VoiceoverView() {
	const t = useT();
	const editor = useEditor();
	const [script, setScript] = useState("");
	const [voice, setVoice] = useState<string>(DEFAULT_KOKORO_VOICE);
	const [speed, setSpeed] = useState(1);
	const [captions, setCaptions] = useState(true);
	const [busy, setBusy] = useState(false);
	const [step, setStep] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const lineCount = splitScript({ script }).length;

	const run = async () => {
		setBusy(true);
		setStatus(null);
		setStep(t("Loading voice..."));
		try {
			const result = await generateVoiceover({
				editor,
				script,
				voice,
				speed,
				captions,
				onProgress: (progress) =>
					setStep(
						progress.step === "loading"
							? t("Loading voice {percent}%", { percent: Math.round(progress.fraction * 100) })
							: progress.step === "speaking"
								? t("Speaking line {done} of {total}", { done: progress.done, total: progress.total })
								: t("Saving..."),
					),
			});
			setStatus(
				t("Added {seconds} s of voice-over at the playhead ({lines} lines).", {
					seconds: result.seconds.toFixed(1),
					lines: result.lines,
				}),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Voice-over failed"));
		} finally {
			setBusy(false);
			setStep(null);
		}
	};

	return (
		<PanelView title={t("Voice-over")}>
			<div className="flex flex-col gap-3" data-testid="voiceover-view">
				<Textarea
					value={script}
					onChange={(event) => setScript(event.target.value)}
					placeholder={t("Type or paste the script. A blank line starts a new paragraph (a longer pause).")}
					className="min-h-32 text-sm"
					aria-label={t("Script")}
					data-testid="voiceover-script"
				/>
				<div className="flex items-center gap-2">
					<Select value={voice} onValueChange={setVoice}>
						<SelectTrigger aria-label={t("Voice")}>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{KOKORO_VOICES.map((id) => (
								<SelectItem key={id} value={id}>
									{voiceLabel({ id, t })}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<Select value={String(speed)} onValueChange={(value) => setSpeed(Number(value))}>
						<SelectTrigger aria-label={t("Speed")} className="w-24">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{SPEEDS.map((value) => (
								<SelectItem key={value} value={String(value)}>
									{value}×
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</div>
				<label className="flex items-center justify-between gap-2 text-xs">
					<span>{t("Add captions, one per line")}</span>
					<Switch checked={captions} onCheckedChange={setCaptions} />
				</label>
				<p className="text-muted-foreground text-xs">
					{t("Spoken on this device by Kokoro (English voices; about 90 MB the first time). Its pronunciation step uses eSpeak NG, which is GPL-3.0.")}
				</p>
				<Button onClick={run} disabled={busy || lineCount === 0} data-testid="voiceover-run">
					{busy && <Spinner className="mr-1" />}
					{busy
						? step
						: lineCount === 0
							? t("Generate voice-over")
							: t("Generate voice-over ({count} lines)", { count: lineCount })}
				</Button>
				{status && (
					<p className="text-muted-foreground text-xs" role="status" data-testid="voiceover-status">
						{status}
					</p>
				)}
			</div>
		</PanelView>
	);
}

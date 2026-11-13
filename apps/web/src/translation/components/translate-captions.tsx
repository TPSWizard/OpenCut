"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useEditor } from "@/editor/use-editor";
import { currentLocale, useT } from "@/i18n";
import { listCaptionTracks, translateCaptionTrack } from "../captions";
import { TRANSLATION_LANGUAGE_CODES, languageName, opusRoute } from "../languages";

/** Translates a caption track into a new track above it, on this device. */
export function TranslateCaptions() {
	const t = useT();
	const editor = useEditor();
	// A primitive key, so a fresh array cannot re-render us forever.
	const tracksKey = useEditor((e) =>
		listCaptionTracks({ editor: e })
			.map((info) => `${info.track.id}:${info.captions}`)
			.join(","),
	);
	const tracks = listCaptionTracks({ editor });
	const [trackId, setTrackId] = useState<string | null>(null);
	const [from, setFrom] = useState("en");
	const [to, setTo] = useState("zh");
	const [busy, setBusy] = useState(false);
	const [step, setStep] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);
	const locale = currentLocale() === "zh-CN" ? "zh-CN" : "en";
	const chosen = tracks.find((info) => info.track.id === trackId) ?? tracks[0];
	const route = opusRoute({ from, to });
	const name = (code: string) => languageName({ code, locale });

	const run = async () => {
		if (!chosen) return;
		setBusy(true);
		setStatus(null);
		setStep(t("Translating..."));
		try {
			const result = await translateCaptionTrack({
				editor,
				trackId: chosen.track.id,
				from,
				to,
				onProgress: (progress) =>
					setStep(
						progress.step === "downloading"
							? t("Loading model {percent}%", { percent: Math.round(progress.fraction * 100) })
							: t("Translating {done} of {total}", { done: progress.done, total: progress.total }),
					),
			});
			setStatus(
				result.engine === "browser"
					? t("{count} caption(s) translated by the browser's built-in translator, on a new track above.", {
							count: result.captions,
						})
					: t("{count} caption(s) translated with {models}, on a new track above.", {
							count: result.captions,
							models: result.models.map((model) => model.replace("Xenova/", "")).join(" → "),
						}),
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : t("Translation failed"));
		} finally {
			setBusy(false);
			setStep(null);
		}
	};

	return (
		<div className="flex flex-col gap-2 rounded-md border p-3" data-testid="translate-captions" data-tracks={tracksKey}>
			<span className="text-sm font-medium">{t("Translate captions")}</span>
			{tracks.length === 0 ? (
				<p className="text-muted-foreground text-xs">
					{t("Generate or import captions first; a translated copy goes on a new track above them.")}
				</p>
			) : (
				<>
					{tracks.length > 1 && (
						<Select value={chosen?.track.id} onValueChange={setTrackId}>
							<SelectTrigger aria-label={t("Caption track")}>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{tracks.map((info) => (
									<SelectItem key={info.track.id} value={info.track.id}>
										{t("{name} ({count} captions)", { name: info.track.name, count: info.captions })}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					)}
					<div className="flex items-center gap-2">
						<Select value={from} onValueChange={setFrom}>
							<SelectTrigger aria-label={t("From")} data-testid="translate-from">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{TRANSLATION_LANGUAGE_CODES.map((code) => (
									<SelectItem key={code} value={code}>
										{name(code)}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<span className="text-muted-foreground text-xs">→</span>
						<Select value={to} onValueChange={setTo}>
							<SelectTrigger aria-label={t("To")} data-testid="translate-to">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{TRANSLATION_LANGUAGE_CODES.filter((code) => code !== from).map((code) => (
									<SelectItem key={code} value={code}>
										{name(code)}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<p className="text-muted-foreground text-xs">
						{route === null
							? t("No translation model covers this pair; Chrome's built-in translator may, where it is available.")
							: route.length > 1
								? t("Goes through English (two models, about 110 MB each the first time), unless the browser translates it itself.")
								: t("Runs on this device: the browser's built-in translator when it has the pair, else an Opus-MT model (about 110 MB the first time).")}
					</p>
					<Button size="sm" onClick={run} disabled={busy || from === to} data-testid="translate-run">
						{busy && <Spinner className="mr-1" />}
						{busy ? step : t("Translate")}
					</Button>
				</>
			)}
			{status && (
				<p className="text-muted-foreground text-xs" role="status" data-testid="translate-status">
					{status}
				</p>
			)}
		</div>
	);
}

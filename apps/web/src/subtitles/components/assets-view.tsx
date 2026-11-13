import { Button } from "@/components/ui/button";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { useReducer, useRef, useState } from "react";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { TRANSCRIPTION_DIAGNOSTICS_SCOPE } from "@/transcription/diagnostics";
import { TRANSCRIPTION_LANGUAGES } from "@/transcription/supported-languages";
import type {
	CaptionChunk,
	TranscriptionLanguage,
	TranscriptionModelId,
} from "@/transcription/types";
import {
	DEFAULT_TRANSCRIPTION_MODEL,
	TRANSCRIPTION_MODELS,
	isTranscriptionModelId,
} from "@/transcription/models";
import { DEFAULT_WORDS_PER_CAPTION } from "@/transcription/caption-defaults";
import { insertCaptionChunksAsTextTrack } from "@/subtitles/insert";
import {
	GENERATED_CAPTION_HIGHLIGHT,
	isCaptionHighlight,
	type CaptionHighlight,
} from "@/captions";
import { generateCaptions, type GenerateCaptionsStep } from "@/captions/generate";
import { Input } from "@/components/ui/input";
import { parseSubtitleFile } from "@/subtitles/parse";
import {
	SUBTITLE_FORMATS,
	collectCaptionCues,
	isSubtitleFormat,
	subtitleFileName,
	writeSubtitles,
	type SubtitleFormat,
} from "@/subtitles/write";
import { downloadBuffer } from "@/export";
import { Spinner } from "@/components/ui/spinner";
import {
	Section,
	SectionContent,
	SectionField,
	SectionFields,
} from "@/components/section";
import { AlertCircleIcon, CloudUploadIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { TranslateCaptions } from "@/translation/components/translate-captions";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { DiagnosticSeverity } from "@/diagnostics/types";

const DIAGNOSTIC_BUTTON_VARIANT: Record<
	DiagnosticSeverity,
	"caution" | "destructive-foreground"
> = {
	caution: "caution",
	error: "destructive-foreground",
};

type ProcessingState =
	| { status: "idle"; error: string | null; warnings: string[] }
	| { status: "processing"; step: string };

type ProcessingAction =
	| { type: "start"; step: string }
	| { type: "update_step"; step: string }
	| { type: "succeed"; warnings: string[] }
	| { type: "fail"; error: string };

const IDLE_STATE: ProcessingState = {
	status: "idle",
	error: null,
	warnings: [],
};

/* eslint-disable opencut/prefer-object-params -- React reducers must accept (state, action). */
function processingReducer(
	state: ProcessingState,
	action: ProcessingAction,
): ProcessingState {
	switch (action.type) {
		case "start":
			return { status: "processing", step: action.step };
		case "update_step":
			if (state.status !== "processing") return state;
			return { status: "processing", step: action.step };
		case "succeed":
			return { status: "idle", error: null, warnings: action.warnings };
		case "fail":
			return { status: "idle", error: action.error, warnings: [] };
	}
}
/* eslint-enable opencut/prefer-object-params */

export function Captions() {
	const t = useT();
	const [selectedLanguage, setSelectedLanguage] =
		useState<TranscriptionLanguage>("auto");
	const [modelId, setModelId] = useState<TranscriptionModelId>(
		DEFAULT_TRANSCRIPTION_MODEL,
	);
	const [wordsPerCaption, setWordsPerCaption] = useState(
		DEFAULT_WORDS_PER_CAPTION,
	);
	const [highlight, setHighlight] = useState<CaptionHighlight>(
		GENERATED_CAPTION_HIGHLIGHT,
	);
	const [processing, dispatch] = useReducer(processingReducer, IDLE_STATE);
	const [exportFormat, setExportFormat] = useState<SubtitleFormat>("srt");
	const [exportNote, setExportNote] = useState<string | null>(null);

	const handleExportCaptions = () => {
		const project = editor.project.getActive();
		const scene = editor.scenes.getActiveScene();
		const tracks = [...scene.tracks.overlay, scene.tracks.main, ...scene.tracks.audio];
		const cues = collectCaptionCues({ tracks });
		if (cues.length === 0) {
			setExportNote(
				t("No captions on the timeline: text elements on visible text tracks are exported."),
			);
			return;
		}
		const content = writeSubtitles({
			format: exportFormat,
			cues,
			canvasSize: project.settings.canvasSize,
			title: project.metadata.name,
		});
		const fileName = subtitleFileName({ name: project.metadata.name, format: exportFormat });
		downloadBuffer({
			buffer: new TextEncoder().encode(content).buffer as ArrayBuffer,
			filename: fileName,
			mimeType: exportFormat === "vtt" ? "text/vtt" : "text/plain",
		});
		setExportNote(t("Saved {count} caption(s) as {fileName}.", { count: cues.length, fileName }));
	};
	const containerRef = useRef<HTMLDivElement>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const editor = useEditor();

	const isProcessing = processing.status === "processing";

	const activeDiagnostics = useEditor((e) =>
		e.diagnostics.getActive({ scope: TRANSCRIPTION_DIAGNOSTICS_SCOPE }),
	);

	const handleStep = (step: GenerateCaptionsStep) => {
		switch (step.step) {
			case "extracting-audio":
				dispatch({ type: "update_step", step: t("Extracting audio...") });
				break;
			case "preparing-audio":
				dispatch({ type: "update_step", step: t("Preparing audio...") });
				break;
			case "loading-model":
				dispatch({
					type: "update_step",
					step: t("Loading model {percent}%", { percent: Math.round(step.progress) }),
				});
				break;
			case "transcribing":
				dispatch({ type: "update_step", step: t("Transcribing...") });
				break;
			case "building-captions":
				dispatch({ type: "update_step", step: t("Generating captions...") });
				break;
		}
	};

	const insertCaptions = ({
		captions,
	}: {
		captions: CaptionChunk[];
	}): boolean => {
		const trackId = insertCaptionChunksAsTextTrack({
			editor,
			captions,
			captionStyle: { highlight },
		});
		return trackId !== null;
	};

	const handleGenerateTranscript = async () => {
		dispatch({ type: "start", step: t("Extracting audio...") });
		try {
			const { result, captions } = await generateCaptions({
				editor,
				language: selectedLanguage,
				modelId,
				wordsPerCaption,
				onStep: handleStep,
			});

			if (!insertCaptions({ captions })) {
				dispatch({ type: "fail", error: t("No captions were generated") });
				return;
			}

			dispatch({
				type: "succeed",
				warnings:
					result.words.length === 0
						? [
								t(
									"This model gave no word timings, so the captions are cut evenly and cannot highlight the spoken word.",
								),
							]
						: [],
			});
		} catch (error) {
			console.error("Transcription failed:", error);
			dispatch({
				type: "fail",
				error:
					error instanceof Error
						? error.message
						: t("An unexpected error occurred"),
			});
		}
	};

	const handleImportClick = () => {
		fileInputRef.current?.click();
	};

	const handleImportFile = async ({ file }: { file: File }) => {
		dispatch({ type: "start", step: t("Reading subtitle file...") });
		try {
			const input = await file.text();
			const result = parseSubtitleFile({
				fileName: file.name,
				input,
			});

			if (result.captions.length === 0) {
				dispatch({
					type: "fail",
					error: t("No valid subtitle cues were found in the subtitle file"),
				});
				return;
			}

			dispatch({ type: "update_step", step: t("Importing subtitles...") });

			if (!insertCaptions({ captions: result.captions })) {
				dispatch({ type: "fail", error: t("No captions were generated") });
				return;
			}

			const nextWarnings = [...result.warnings];
			if (result.skippedCueCount > 0) {
				nextWarnings.unshift(
					t("Imported {imported} subtitle cue(s) and skipped {skipped} malformed cue(s).", {
						imported: result.captions.length,
						skipped: result.skippedCueCount,
					}),
				);
			}

			dispatch({ type: "succeed", warnings: nextWarnings });
		} catch (error) {
			console.error("Subtitle import failed:", error);
			dispatch({
				type: "fail",
				error:
					error instanceof Error
						? error.message
						: t("An unexpected error occurred"),
			});
		}
	};

	const handleFileChange = async ({
		event,
	}: {
		event: React.ChangeEvent<HTMLInputElement>;
	}) => {
		const file = event.target.files?.[0];
		if (event.target) {
			event.target.value = "";
		}
		if (!file) return;

		await handleImportFile({ file });
	};

	const handleLanguageChange = ({ value }: { value: string }) => {
		if (value === "auto") {
			setSelectedLanguage("auto");
			return;
		}

		const matchedLanguage = TRANSCRIPTION_LANGUAGES.find(
			(language) => language.code === value,
		);
		if (!matchedLanguage) return;
		setSelectedLanguage(matchedLanguage.code);
	};

	const error = processing.status === "idle" ? processing.error : null;
	const warnings = processing.status === "idle" ? processing.warnings : [];

	return (
		<PanelView
			title={t("Captions")}
			contentClassName="px-0 flex flex-col h-full"
			actions={
				<TooltipProvider>
					<div className="flex items-center gap-1.5">
						{!isProcessing &&
							activeDiagnostics.map((diagnostic) => (
								<Tooltip key={diagnostic.id}>
									<TooltipTrigger asChild>
										<Button
											variant={DIAGNOSTIC_BUTTON_VARIANT[diagnostic.severity]}
											size="icon"
											aria-label={diagnostic.message}
										>
											<HugeiconsIcon icon={AlertCircleIcon} size={16} />
										</Button>
									</TooltipTrigger>
									<TooltipContent>{diagnostic.message}</TooltipContent>
								</Tooltip>
							))}
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={handleImportClick}
							disabled={isProcessing}
							className="items-center justify-center gap-1.5"
						>
							<HugeiconsIcon icon={CloudUploadIcon} />
							{t("Import")}
						</Button>
					</div>
				</TooltipProvider>
			}
			ref={containerRef}
		>
			<input
				ref={fileInputRef}
				type="file"
				accept=".srt,.ass"
				className="hidden"
				onChange={(event) => void handleFileChange({ event })}
			/>
			<Section
				showTopBorder={false}
				showBottomBorder={false}
				className="flex-1"
			>
				<SectionContent className="flex flex-col gap-4 h-full pt-1">
					<SectionFields>
						<SectionField label={t("Language")}>
							<Select
								value={selectedLanguage}
								onValueChange={(value) => handleLanguageChange({ value })}
							>
								<SelectTrigger>
									<SelectValue placeholder={t("Select a language")} />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="auto">{t("Auto detect")}</SelectItem>
									{TRANSCRIPTION_LANGUAGES.map((language) => (
										<SelectItem key={language.code} value={language.code}>
											{language.name}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</SectionField>
						<SectionField label={t("Model")}>
							<Select
								value={modelId}
								onValueChange={(value) => {
									if (isTranscriptionModelId(value)) setModelId(value);
								}}
							>
								<SelectTrigger>
									<SelectValue placeholder={t("Select a model")} />
								</SelectTrigger>
								<SelectContent>
									{TRANSCRIPTION_MODELS.map((model) => (
										<SelectItem key={model.id} value={model.id}>
											{model.name} · {model.description}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</SectionField>
						<SectionField label={t("Words per caption")}>
							<Input
								type="number"
								min={1}
								max={12}
								step={1}
								value={wordsPerCaption}
								onChange={(event) => {
									const next = Number.parseInt(event.target.value, 10);
									if (Number.isFinite(next) && next >= 1) {
										setWordsPerCaption(Math.min(12, next));
									}
								}}
								aria-label={t("Words per caption")}
							/>
						</SectionField>
						<SectionField label={t("Highlight")}>
							<Select
								value={highlight}
								onValueChange={(value) => {
									if (isCaptionHighlight(value)) setHighlight(value);
								}}
							>
								<SelectTrigger>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="none">{t("None")}</SelectItem>
									<SelectItem value="word">{t("Active word")}</SelectItem>
									<SelectItem value="box">{t("Active word in a box")}</SelectItem>
									<SelectItem value="fill">{t("Spoken so far (karaoke)")}</SelectItem>
								</SelectContent>
							</Select>
						</SectionField>
						<SectionField label={t("Export")}>
							<div className="flex items-center gap-2">
								<Select
									value={exportFormat}
									onValueChange={(value) => {
										if (isSubtitleFormat(value)) setExportFormat(value);
									}}
								>
									<SelectTrigger aria-label={t("Subtitle format")}>
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										{SUBTITLE_FORMATS.map((format) => (
											<SelectItem key={format.id} value={format.id}>
												{format.label}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
								<Button
									type="button"
									variant="outline"
									size="sm"
									onClick={handleExportCaptions}
									disabled={isProcessing}
								>
									{t("Download")}
								</Button>
							</div>
						</SectionField>
					</SectionFields>
					{exportNote && (
						<p className="text-xs text-muted-foreground" role="status">
							{exportNote}
						</p>
					)}
					<TranslateCaptions />

					<Button
						type="button"
						className="mt-auto w-full"
						onClick={handleGenerateTranscript}
						disabled={isProcessing || activeDiagnostics.length > 0}
					>
						{isProcessing && <Spinner className="mr-1" />}
						{isProcessing ? processing.step : t("Generate transcript")}
					</Button>
					{error && (
						<div className="bg-destructive/10 border-destructive/20 rounded-md border p-3">
							<p className="text-destructive text-sm">{error}</p>
						</div>
					)}
					{warnings.length > 0 && (
						<div className="rounded-md border border-amber-500/20 bg-amber-500/10 p-3">
							<ul className="space-y-1 text-sm text-amber-700">
								{warnings.map((warning) => (
									<li key={warning}>{warning}</li>
								))}
							</ul>
						</div>
					)}
				</SectionContent>
			</Section>
		</PanelView>
	);
}

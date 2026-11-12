/**
 * Writing the captions of a scene out as subtitle files: SubRip (`.srt`),
 * WebVTT (`.vtt`, with inline timestamps for word timing) and Advanced
 * SubStation Alpha (`.ass`, with `\k` karaoke tags). The desktop's
 * `editor::subtitles` writes the same bytes for the same captions.
 */

import type { TextElement, TimelineTrack } from "@/timeline/types";
import { mediaTimeToSeconds, type MediaTime } from "@/wasm";
import { FONT_SIZE_SCALE_REFERENCE } from "@/text/typography";

export type SubtitleFormat = "srt" | "vtt" | "ass";

export const SUBTITLE_FORMATS: { id: SubtitleFormat; label: string; extension: string }[] = [
	{ id: "srt", label: "SubRip (.srt)", extension: "srt" },
	{ id: "vtt", label: "WebVTT (.vtt)", extension: "vtt" },
	{ id: "ass", label: "SubStation Alpha (.ass)", extension: "ass" },
];

export function isSubtitleFormat(value: unknown): value is SubtitleFormat {
	return value === "srt" || value === "vtt" || value === "ass";
}

/** One word of a cue, in seconds on the timeline clock. */
export interface ExportWord {
	text: string;
	start: number;
	end: number;
}

/** A caption ready to be written: times in seconds on the timeline clock. */
export interface ExportCue {
	text: string;
	start: number;
	end: number;
	/** Word timings when the caption carries them; joined by spaces they spell `text`. */
	words?: ExportWord[];
	style: ExportCueStyle;
}

/** The parts of a text element's look that a subtitle style line can carry. */
export interface ExportCueStyle {
	fontFamily: string;
	/** In app units (the same space as `TextElement.params.fontSize`): pixels are this times the canvas height over 90. */
	fontSize: number;
	color: string;
	bold: boolean;
	italic: boolean;
	textAlign: "left" | "center" | "right";
	/** Offset of the text's centre from the canvas centre, in canvas pixels (y down). */
	positionY: number;
}

function toSeconds(time: MediaTime): number {
	return mediaTimeToSeconds({ time });
}

/**
 * The caption cues of the given tracks (text elements, visible, in start
 * order), optionally only those of one track.
 */
export function collectCaptionCues({
	tracks,
	trackId,
}: {
	tracks: TimelineTrack[];
	trackId?: string;
}): ExportCue[] {
	const cues: ExportCue[] = [];
	for (const track of tracks) {
		if (track.type !== "text") continue;
		if (trackId && track.id !== trackId) continue;
		if (track.hidden) continue;
		for (const element of track.elements as TextElement[]) {
			if (element.type !== "text" || element.hidden) continue;
			const content = String(element.params.content ?? "").trim();
			if (!content) continue;
			const start = toSeconds(element.startTime);
			const end = start + toSeconds(element.duration);
			const params = element.params as Record<string, unknown>;
			const textAlign = params.textAlign;
			const words = element.words?.map((word) => ({
				text: word.text,
				start: start + toSeconds(word.start),
				end: start + toSeconds(word.end),
			}));
			cues.push({
				text: content,
				start,
				end,
				...(words && words.length > 0 ? { words } : {}),
				style: {
					fontFamily: String(params.fontFamily ?? "Arial"),
					fontSize: Number(params.fontSize ?? 15),
					color: String(params.color ?? "#ffffff"),
					bold: params.fontWeight === "bold" || Number(params.fontWeight) >= 600,
					italic: params.fontStyle === "italic",
					textAlign: textAlign === "left" || textAlign === "right" ? textAlign : "center",
					positionY: Number(params["transform.positionY"] ?? 0),
				},
			});
		}
	}
	cues.sort((a, b) => a.start - b.start || a.end - b.end);
	return cues;
}

export function writeSubtitles({
	format,
	cues,
	canvasSize,
	title,
}: {
	format: SubtitleFormat;
	cues: ExportCue[];
	canvasSize: { width: number; height: number };
	title?: string;
}): string {
	switch (format) {
		case "srt":
			return writeSrt({ cues });
		case "vtt":
			return writeVtt({ cues });
		case "ass":
			return writeAss({ cues, canvasSize, title });
	}
}

/** `HH:MM:SS` plus milliseconds with the given separator. */
function clockTime({ seconds, separator }: { seconds: number; separator: string }): string {
	const total = Math.max(0, Math.round(seconds * 1000));
	const ms = total % 1000;
	const s = Math.floor(total / 1000) % 60;
	const m = Math.floor(total / 60_000) % 60;
	const h = Math.floor(total / 3_600_000);
	return `${pad(h, 2)}:${pad(m, 2)}:${pad(s, 2)}${separator}${pad(ms, 3)}`;
}

function pad(value: number, width: number): string {
	return String(value).padStart(width, "0");
}

export function writeSrt({ cues }: { cues: ExportCue[] }): string {
	const blocks = cues.map((cue, index) => {
		const start = clockTime({ seconds: cue.start, separator: "," });
		const end = clockTime({ seconds: cue.end, separator: "," });
		return `${index + 1}\n${start} --> ${end}\n${cue.text.replace(/\r\n?/g, "\n")}`;
	});
	return blocks.length > 0 ? `${blocks.join("\n\n")}\n` : "";
}

function escapeVtt(text: string): string {
	return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * WebVTT; a cue with word timings gets an inline timestamp before each word
 * after the first (`word <00:00:01.200>word`), which players use to style
 * the words spoken so far.
 */
export function writeVtt({ cues }: { cues: ExportCue[] }): string {
	const blocks = cues.map((cue, index) => {
		const start = clockTime({ seconds: cue.start, separator: "." });
		const end = clockTime({ seconds: cue.end, separator: "." });
		const body = cue.words
			? cue.words
					.map((word, wordIndex) =>
						wordIndex === 0
							? escapeVtt(word.text)
							: `<${clockTime({ seconds: word.start, separator: "." })}>${escapeVtt(word.text)}`,
					)
					.join(" ")
			: escapeVtt(cue.text.replace(/\r\n?/g, "\n"));
		return `${index + 1}\n${start} --> ${end}\n${body}`;
	});
	return `WEBVTT\n\n${blocks.join("\n\n")}${blocks.length > 0 ? "\n" : ""}`;
}

/** `H:MM:SS.cc` as ASS writes times. */
function assTime(seconds: number): string {
	const total = Math.max(0, Math.round(seconds * 100));
	const cs = total % 100;
	const s = Math.floor(total / 100) % 60;
	const m = Math.floor(total / 6000) % 60;
	const h = Math.floor(total / 360_000);
	return `${h}:${pad(m, 2)}:${pad(s, 2)}.${pad(cs, 2)}`;
}

/** `&HAABBGGRR` from `#rrggbb` or `#rrggbbaa` (ASS alpha is 00 opaque). */
export function assColour(css: string): string {
	const hex = css.trim().replace(/^#/, "");
	const long =
		hex.length === 3 || hex.length === 4
			? hex
					.split("")
					.map((c) => c + c)
					.join("")
			: hex;
	if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(long)) {
		return "&H00FFFFFF";
	}
	const r = long.slice(0, 2);
	const g = long.slice(2, 4);
	const b = long.slice(4, 6);
	const alpha = long.length === 8 ? (255 - Number.parseInt(long.slice(6, 8), 16)) : 0;
	return `&H${alpha.toString(16).padStart(2, "0")}${b}${g}${r}`.toUpperCase();
}

function assEscape(text: string): string {
	return text.replace(/\r\n?/g, "\n").replace(/\n/g, "\\N").replace(/\{/g, "(").replace(/\}/g, ")");
}

/**
 * ASS alignment (numpad layout) and vertical margin for a caption placed
 * `positionY` canvas pixels below the centre: the lower third is 1–3, the
 * upper third 7–9, the rest 4–6.
 */
export function assPlacement({
	style,
	canvasHeight,
}: {
	style: ExportCueStyle;
	canvasHeight: number;
}): { alignment: number; marginV: number } {
	const column = style.textAlign === "left" ? 1 : style.textAlign === "right" ? 3 : 2;
	const third = canvasHeight / 6;
	const lineHalf = (style.fontSize * (canvasHeight / FONT_SIZE_SCALE_REFERENCE) * 1.2) / 2;
	if (style.positionY > third) {
		return {
			alignment: column,
			marginV: Math.max(0, Math.round(canvasHeight / 2 - style.positionY - lineHalf)),
		};
	}
	if (style.positionY < -third) {
		return {
			alignment: 6 + column,
			marginV: Math.max(0, Math.round(canvasHeight / 2 + style.positionY - lineHalf)),
		};
	}
	return { alignment: 3 + column, marginV: 0 };
}

/**
 * ASS with one style per distinct look and `\k` karaoke tags on cues that
 * carry words (each tag holds the word's duration in centiseconds; a pause
 * before a word becomes an empty syllable).
 */
export function writeAss({
	cues,
	canvasSize,
	title,
}: {
	cues: ExportCue[];
	canvasSize: { width: number; height: number };
	title?: string;
}): string {
	const styles: { name: string; line: string }[] = [];
	const styleNames = new Map<string, string>();
	const styleFor = (style: ExportCueStyle): string => {
		const { alignment, marginV } = assPlacement({ style, canvasHeight: canvasSize.height });
		const fontSize = Math.round(style.fontSize * (canvasSize.height / FONT_SIZE_SCALE_REFERENCE));
		const key = [
			style.fontFamily,
			fontSize,
			style.color,
			style.bold,
			style.italic,
			alignment,
			marginV,
		].join("|");
		const existing = styleNames.get(key);
		if (existing) return existing;
		const name = styles.length === 0 ? "Default" : `Style${styles.length + 1}`;
		const primary = assColour(style.color);
		const line = [
			name,
			style.fontFamily.replace(/,/g, " "),
			String(fontSize),
			primary,
			"&H00FFFF00",
			"&H00000000",
			"&H80000000",
			style.bold ? "-1" : "0",
			style.italic ? "-1" : "0",
			"0",
			"0",
			"100",
			"100",
			"0",
			"0",
			"1",
			"2",
			"1",
			String(alignment),
			"20",
			"20",
			String(marginV),
			"1",
		].join(",");
		styles.push({ name, line });
		styleNames.set(key, name);
		return name;
	};

	const events = cues.map((cue) => {
		const style = styleFor(cue.style);
		const start = assTime(cue.start);
		const end = assTime(cue.end);
		let body: string;
		if (cue.words && cue.words.length > 0) {
			const parts: string[] = [];
			let cursor = cue.start;
			for (const [index, word] of cue.words.entries()) {
				const gap = Math.round((word.start - cursor) * 100);
				if (gap > 0) {
					parts.push(`{\\k${gap}}`);
				}
				const duration = Math.max(1, Math.round((Math.max(word.end, word.start) - Math.max(word.start, cursor)) * 100));
				parts.push(`{\\k${duration}}${assEscape(word.text)}${index + 1 < cue.words.length ? " " : ""}`);
				cursor = Math.max(cursor, word.end);
			}
			body = parts.join("");
		} else {
			body = assEscape(cue.text);
		}
		return `Dialogue: 0,${start},${end},${style},,0,0,0,,${body}`;
	});

	const lines = [
		"[Script Info]",
		`Title: ${(title ?? "Captions").replace(/\r?\n/g, " ")}`,
		"ScriptType: v4.00+",
		`PlayResX: ${canvasSize.width}`,
		`PlayResY: ${canvasSize.height}`,
		"WrapStyle: 0",
		"ScaledBorderAndShadow: yes",
		"",
		"[V4+ Styles]",
		"Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding",
		...styles.map((style) => `Style: ${style.line}`),
		"",
		"[Events]",
		"Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
		...events,
	];
	if (styles.length === 0) {
		lines.splice(
			lines.indexOf("[V4+ Styles]") + 2,
			0,
			"Style: Default,Arial,48,&H00FFFFFF,&H00FFFF00,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,20,20,40,1",
		);
	}
	return `${lines.join("\n")}\n`;
}

export function subtitleFileName({ name, format }: { name: string; format: SubtitleFormat }): string {
	const cleaned = name.replace(/[^\p{L}\p{N} _-]+/gu, "_").trim() || "captions";
	return `${cleaned}.${format}`;
}

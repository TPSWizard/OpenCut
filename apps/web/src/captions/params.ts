import type { ParamValues } from "@/params";
import { isCaptionHighlight, type CaptionHighlight, type CaptionStyle } from "./types";

export const CAPTION_HIGHLIGHT_PARAM = "caption.highlight";
export const CAPTION_ACTIVE_COLOR_PARAM = "caption.activeColor";
export const CAPTION_BOX_COLOR_PARAM = "caption.boxColor";
export const CAPTION_KEYWORD_COLOR_PARAM = "caption.keywordColor";
export const CAPTION_KEYWORD_SCALE_PARAM = "caption.keywordScale";
/** The largest keyword size; `KEYWORD_SCALE_MAX` in `crates/editor/src/caption_keywords.rs`. */
export const KEYWORD_SCALE_MAX = 2;

export const CAPTION_PARAM_KEYS = [
	CAPTION_HIGHLIGHT_PARAM,
	CAPTION_ACTIVE_COLOR_PARAM,
	CAPTION_BOX_COLOR_PARAM,
	CAPTION_KEYWORD_COLOR_PARAM,
	CAPTION_KEYWORD_SCALE_PARAM,
] as const;

/** Keyword colour and scale match `KEYWORD_COLOR_DEFAULT` / `KEYWORD_SCALE_DEFAULT` in the Rust editor. */
export const DEFAULT_CAPTION_STYLE: CaptionStyle = {
	highlight: "none",
	activeColor: "#ffd60a",
	boxColor: "#e5484d",
	keywordColor: "#3ee07a",
	keywordScale: 1.25,
};

/** The default look for captions the editor generates itself. */
export const GENERATED_CAPTION_HIGHLIGHT: CaptionHighlight = "word";

export function readCaptionStyle({
	params,
}: {
	params: ParamValues;
}): CaptionStyle {
	const highlight = params[CAPTION_HIGHLIGHT_PARAM];
	const activeColor = params[CAPTION_ACTIVE_COLOR_PARAM];
	const boxColor = params[CAPTION_BOX_COLOR_PARAM];
	const keywordColor = params[CAPTION_KEYWORD_COLOR_PARAM];
	const keywordScale = params[CAPTION_KEYWORD_SCALE_PARAM];
	return {
		highlight: isCaptionHighlight(highlight)
			? highlight
			: DEFAULT_CAPTION_STYLE.highlight,
		activeColor:
			typeof activeColor === "string"
				? activeColor
				: DEFAULT_CAPTION_STYLE.activeColor,
		boxColor:
			typeof boxColor === "string" ? boxColor : DEFAULT_CAPTION_STYLE.boxColor,
		keywordColor:
			typeof keywordColor === "string" ? keywordColor : DEFAULT_CAPTION_STYLE.keywordColor,
		keywordScale:
			typeof keywordScale === "number" && Number.isFinite(keywordScale)
				? Math.min(KEYWORD_SCALE_MAX, Math.max(1, keywordScale))
				: DEFAULT_CAPTION_STYLE.keywordScale,
	};
}

export function captionStyleParams({
	style,
}: {
	style: Partial<CaptionStyle>;
}): ParamValues {
	const values: ParamValues = {};
	if (style.highlight !== undefined) {
		values[CAPTION_HIGHLIGHT_PARAM] = style.highlight;
	}
	if (style.activeColor !== undefined) {
		values[CAPTION_ACTIVE_COLOR_PARAM] = style.activeColor;
	}
	if (style.boxColor !== undefined) {
		values[CAPTION_BOX_COLOR_PARAM] = style.boxColor;
	}
	if (style.keywordColor !== undefined) {
		values[CAPTION_KEYWORD_COLOR_PARAM] = style.keywordColor;
	}
	if (style.keywordScale !== undefined) {
		values[CAPTION_KEYWORD_SCALE_PARAM] = style.keywordScale;
	}
	return values;
}

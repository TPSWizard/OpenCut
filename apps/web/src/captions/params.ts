import type { ParamValues } from "@/params";
import { isCaptionHighlight, type CaptionHighlight, type CaptionStyle } from "./types";

export const CAPTION_HIGHLIGHT_PARAM = "caption.highlight";
export const CAPTION_ACTIVE_COLOR_PARAM = "caption.activeColor";
export const CAPTION_BOX_COLOR_PARAM = "caption.boxColor";

export const CAPTION_PARAM_KEYS = [
	CAPTION_HIGHLIGHT_PARAM,
	CAPTION_ACTIVE_COLOR_PARAM,
	CAPTION_BOX_COLOR_PARAM,
] as const;

export const DEFAULT_CAPTION_STYLE: CaptionStyle = {
	highlight: "none",
	activeColor: "#ffd60a",
	boxColor: "#e5484d",
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
	return values;
}

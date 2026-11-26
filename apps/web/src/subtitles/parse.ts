import { parseAss } from "./ass";
import { parseSharedSubtitles } from "./shared";
import type { ParseSubtitleResult } from "./types";
export type { ParseSubtitleResult, SubtitleCue } from "./types";

export function parseSubtitleFile({
	fileName,
	input,
}: {
	fileName: string;
	input: string;
}): ParseSubtitleResult {
	const extension = getFileExtension({ fileName });

	switch (extension) {
		case "srt":
		case "vtt":
			return parseSharedSubtitles({ input, fileName });
		case "ass":
			return parseAss({ input });
		default:
			throw new Error("Unsupported subtitle format");
	}
}

function getFileExtension({ fileName }: { fileName: string }): string {
	const extension = fileName.split(".").pop();
	return extension?.toLowerCase() ?? "";
}

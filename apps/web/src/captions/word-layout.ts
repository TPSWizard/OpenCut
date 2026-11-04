/**
 * Maps caption words onto the lines of a laid-out text block. The words of a
 * caption are the whitespace-separated tokens of its content in order, so
 * the k-th word is the k-th token across the lines, whatever the wrapping.
 */
export interface WordPlacement {
	/** Index of the word among the block's tokens. */
	index: number;
	line: number;
	/** The text before the word on its line (used to measure its offset). */
	prefix: string;
	text: string;
}

export function placeWords({
	lines,
}: {
	lines: readonly string[];
}): WordPlacement[] {
	const placements: WordPlacement[] = [];
	let index = 0;
	for (let line = 0; line < lines.length; line++) {
		const text = lines[line];
		const pattern = /\S+/g;
		let match: RegExpExecArray | null = pattern.exec(text);
		while (match) {
			placements.push({
				index,
				line,
				prefix: text.slice(0, match.index),
				text: match[0],
			});
			index += 1;
			match = pattern.exec(text);
		}
	}
	return placements;
}

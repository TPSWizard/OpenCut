import type { MediaTime } from "@/wasm";
import type { ActiveWordState, CaptionWord } from "./types";

/**
 * The word spoken at `localTime` (relative to the element's start) and how
 * many words have started by then. Words are expected in time order; the
 * last word to have started wins when two overlap.
 */
export function getActiveWordState({
	words,
	localTime,
}: {
	words: readonly CaptionWord[];
	localTime: MediaTime;
}): ActiveWordState {
	let activeIndex = -1;
	let spokenCount = 0;
	for (let index = 0; index < words.length; index++) {
		const word = words[index];
		if (word.start > localTime) {
			break;
		}
		spokenCount = index + 1;
		if (localTime < word.end) {
			activeIndex = index;
		}
	}
	return { activeIndex, spokenCount };
}

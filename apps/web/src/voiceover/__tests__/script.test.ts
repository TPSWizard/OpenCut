import { describe, expect, test } from "vitest";
import { layoutLines, splitScript } from "../script";

describe("voice-over script", () => {
	// Same script and expectations as crates/editor/src/voiceover.rs.
	test("scripts split into lines with pauses", () => {
		const script =
			"Welcome back to the channel!  Today we cook rice.\r\n\r\nFirst, wash the rice three times... Then soak it, drain it, and let it rest for twenty minutes while you chop the onions, the garlic, the ginger and the spring onions for the sauce that goes on top.\n\n你好。今天做饭！";
		const lines = splitScript({ script, maxChars: 80 });
		expect(lines.map((line) => line.text)).toEqual([
			"Welcome back to the channel!",
			"Today we cook rice.",
			"First, wash the rice three times...",
			"Then soak it, drain it,",
			"and let it rest for twenty minutes while you chop the onions,",
			"the garlic, the ginger and the spring onions for the sauce that goes on top.",
			"你好。",
			"今天做饭！",
		]);
		expect(lines.map((line) => line.pauseAfter)).toEqual([0.25, 0.6, 0.25, 0.25, 0.25, 0.6, 0.25, 0]);
		expect(splitScript({ script: "  \n\n " })).toEqual([]);
	});

	test("lines are laid out with their pauses", () => {
		const lines = splitScript({ script: "One. Two.\n\nThree." });
		const { spans, total } = layoutLines({ lines, durations: [1, 2, 0.5] });
		expect(spans).toEqual([
			[0, 1],
			[1.25, 3.25],
			[3.85, 4.35],
		]);
		expect(total).toBeCloseTo(4.35, 9);
	});
});

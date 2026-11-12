"use client";

import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useElementPlayhead } from "@/components/editor/panels/properties/hooks/use-element-playhead";
import type { TextElement } from "@/timeline/types";
import { mediaTimeToSeconds } from "@/wasm";
import { cn } from "@/utils/ui";
import { useT } from "@/i18n";
import { CAPTION_PARAM_KEYS } from "../params";
import { getActiveWordState } from "../active-word";

/**
 * A caption's look (how the spoken word stands out) and its words with
 * their timings; the word under the playhead is marked.
 */
export function CaptionsTab({
	element,
	trackId,
}: {
	element: TextElement;
	trackId: string;
}) {
	const t = useT();
	const words = element.words ?? [];
	const { localTime, isPlayheadWithinElementRange } = useElementPlayhead({
		startTime: element.startTime,
		duration: element.duration,
	});
	const state = isPlayheadWithinElementRange
		? getActiveWordState({ words, localTime })
		: { activeIndex: -1, spokenCount: 0 };

	return (
		<div className="flex flex-col">
			<ElementParamsTab
				element={element}
				trackId={trackId}
				paramKeys={CAPTION_PARAM_KEYS}
				sectionKey="captions"
			/>
			<div className="flex flex-col gap-2 px-4 pb-4">
				<p className="text-muted-foreground text-xs">
					{words.length === 1
						? t("1 timed word; times are from the start of the clip.")
						: t("{count} timed words; times are from the start of the clip.", {
								count: words.length,
							})}
				</p>
				<ol className="flex flex-col gap-0.5" aria-label={t("Caption words")}>
					{words.map((word, index) => (
						<li
							key={`${index}:${word.text}`}
							className={cn(
								"flex items-baseline justify-between gap-2 rounded px-1.5 py-0.5 text-xs",
								index === state.activeIndex && "bg-primary/15 text-primary",
							)}
						>
							<span className="truncate font-medium">{word.text}</span>
							<span className="text-muted-foreground tabular-nums">
								{mediaTimeToSeconds({ time: word.start }).toFixed(2)}–
								{mediaTimeToSeconds({ time: word.end }).toFixed(2)}s
							</span>
						</li>
					))}
				</ol>
			</div>
		</div>
	);
}

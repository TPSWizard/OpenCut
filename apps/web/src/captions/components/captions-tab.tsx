"use client";

import { useState } from "react";
import { type CaptionEditOp, countCaptionMatches, editCaptions } from "../find-replace";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useElementPlayhead } from "@/components/editor/panels/properties/hooks/use-element-playhead";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import { rememberPasscode, rememberedPasscode } from "@/project/share";
import type { TextElement } from "@/timeline/types";
import { mediaTimeToSeconds } from "@/wasm";
import { cn } from "@/utils/ui";
import { useT } from "@/i18n";
import { CAPTION_PARAM_KEYS } from "../params";
import { getActiveWordState } from "../active-word";
import {
	clearCaptionKeywords,
	pickCaptionKeywords,
	pickCaptionKeywordsWithClaude,
	toggleCaptionKeyword,
	type KeywordResult,
} from "../keyword-commands";

/**
 * A caption's look (how the spoken word and the keywords stand out), the
 * keyword pick for every caption, and its words with their timings; the
 * word under the playhead is marked and clicking a word makes it a
 * keyword or a plain word again.
 */
export function CaptionsTab({
	element,
	trackId,
}: {
	element: TextElement;
	trackId: string;
}) {
	const t = useT();
	const editor = useEditor();
	const words = element.words ?? [];
	const [withClaude, setWithClaude] = useState(false);
	const [passcode, setPasscode] = useState(() => rememberedPasscode());
	const [busy, setBusy] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const { localTime, isPlayheadWithinElementRange } = useElementPlayhead({
		startTime: element.startTime,
		duration: element.duration,
	});
	const state = isPlayheadWithinElementRange
		? getActiveWordState({ words, localTime })
		: { activeIndex: -1, spokenCount: 0 };

	const report = (result: KeywordResult) =>
		setStatus(
			t("{keywords} keyword(s) in {captions} changed caption(s).", {
				keywords: result.keywords,
				captions: result.captions,
			}),
		);

	const pick = async () => {
		setStatus(null);
		if (!withClaude) {
			report(pickCaptionKeywords({ editor }));
			return;
		}
		setBusy(true);
		try {
			const result = await pickCaptionKeywordsWithClaude({ editor, passcode });
			rememberPasscode({ passcode });
			report(result);
		} catch (error) {
			setStatus(
				`${error instanceof Error ? error.message : String(error)}. ${t("The editor's own pick still works without Claude.")}`,
			);
		} finally {
			setBusy(false);
		}
	};

	return (
		<div className="flex flex-col">
			<ElementParamsTab
				element={element}
				trackId={trackId}
				paramKeys={CAPTION_PARAM_KEYS}
				sectionKey="captions"
			/>
			<div className="flex flex-col gap-2 px-4 pb-4" data-testid="caption-keywords">
				<p className="text-muted-foreground text-xs">
					{t(
						"Keywords are the words that matter in each caption, drawn in the keyword colour and size. Pick them for every caption, or click a word below.",
					)}
				</p>
				<label className="flex items-center gap-2 text-xs">
					<Checkbox
						checked={withClaude}
						onCheckedChange={(value) => setWithClaude(value === true)}
						data-testid="keywords-claude"
					/>
					{t("Pick with Claude (uses the server's key and passcode)")}
				</label>
				{withClaude && (
					<Input
						type="password"
						value={passcode}
						onChange={(event) => setPasscode(event.target.value)}
						placeholder={t("Server passcode")}
						className="h-8 text-xs"
						data-testid="keywords-passcode"
					/>
				)}
				<div className="flex gap-2">
					<Button size="sm" variant="outline" disabled={busy} onClick={() => void pick()} data-testid="keywords-pick">
						{busy ? t("Asking Claude…") : t("Pick keywords")}
					</Button>
					<Button
						size="sm"
						variant="ghost"
						disabled={busy}
						onClick={() => report(clearCaptionKeywords({ editor }))}
						data-testid="keywords-clear"
					>
						{t("Clear keywords")}
					</Button>
				</div>
				{status && (
					<p className="text-muted-foreground text-xs" role="status">
						{status}
					</p>
				)}
			</div>
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
						<li key={`${index}:${word.text}`}>
							<button
								type="button"
								title={word.keyword ? t("Keyword; click to make it a plain word") : t("Click to make it a keyword")}
								aria-pressed={word.keyword === true}
								onClick={() => toggleCaptionKeyword({ editor, trackId, element, index })}
								className={cn(
									"flex w-full items-baseline justify-between gap-2 rounded px-1.5 py-0.5 text-left text-xs hover:bg-accent",
									index === state.activeIndex && "bg-primary/15 text-primary",
								)}
							>
								<span className={cn("truncate font-medium", word.keyword && "text-emerald-500")}>
									{word.keyword ? `★ ${word.text}` : word.text}
								</span>
								<span className="text-muted-foreground tabular-nums">
									{mediaTimeToSeconds({ time: word.start }).toFixed(2)}–
									{mediaTimeToSeconds({ time: word.end }).toFixed(2)}s
								</span>
							</button>
						</li>
					))}
				</ol>
			</div>
			<FindReplaceCaptions />
		</div>
	);
}

/** Find and replace across every caption, highlight matches, and auto emoji (row 94). */
function FindReplaceCaptions() {
	const t = useT();
	const editor = useEditor();
	const [find, setFind] = useState("");
	const [replace, setReplace] = useState("");
	const [status, setStatus] = useState<string | null>(null);
	useEditor((e) => e.scenes.getActiveSceneOrNull()?.tracks);
	const matches = countCaptionMatches({ editor, query: find });
	const run = (op: CaptionEditOp) => {
		const changed = editCaptions({ editor, op, query: find, replacement: replace });
		setStatus(changed > 0 ? t("Changed {count} caption(s).", { count: changed }) : t("Nothing to change in the captions."));
	};
	return (
		<div className="flex flex-col gap-2 border-t px-4 py-3" data-testid="caption-find-replace">
			<p className="text-xs font-medium">{t("Find and replace in captions")}</p>
			<Input className="h-7 text-xs" placeholder={t("Find")} value={find} data-testid="caption-find" onChange={(event) => setFind(event.target.value)} />
			<Input
				className="h-7 text-xs"
				placeholder={t("Replace with")}
				value={replace}
				data-testid="caption-replace"
				onChange={(event) => setReplace(event.target.value)}
			/>
			<p className="text-muted-foreground text-xs" data-testid="caption-match-count">
				{find.trim() ? t("{count} match(es) across the captions.", { count: matches }) : t("Whole words, any case; the words keep their timing.")}
			</p>
			<div className="flex flex-wrap gap-1">
				<Button size="sm" variant="outline" className="h-7 text-xs" disabled={matches === 0} data-testid="caption-replace-all" onClick={() => run("replace")}>
					{t("Replace all")}
				</Button>
				<Button size="sm" variant="outline" className="h-7 text-xs" disabled={matches === 0} onClick={() => run("highlight")}>
					{t("Highlight matches")}
				</Button>
				<Button size="sm" variant="outline" className="h-7 text-xs" data-testid="caption-add-emoji" onClick={() => run("addEmoji")}>
					{t("Add emoji")}
				</Button>
				<Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => run("removeEmoji")}>
					{t("Remove emoji")}
				</Button>
			</div>
			{status && (
				<p className="text-muted-foreground text-xs" role="status">
					{status}
				</p>
			)}
		</div>
	);
}

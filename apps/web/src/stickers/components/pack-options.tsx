"use client";

import { cn } from "@/utils/ui";
import { useStickersStore } from "@/stickers/stickers-store";
import { useStickerPackPrefs, type BrandColorMode, type EmojiSetId } from "@/stickers/pack-prefs";
import { EMOJI_SETS, SKIN_TONES } from "@/stickers/providers/emoji";

/** Re-runs what the panel shows after a pack setting changes. */
function refresh() {
	const store = useStickersStore.getState();
	const query = store.searchQuery.trim();
	if (query) void store.searchStickers({ query });
	else void store.browseStickers();
}

/** Emoji set and skin tone, with the set's licence credit. */
export function EmojiPackOptions() {
	const { emojiSet, skinTone, setEmojiSet, setSkinTone } = useStickerPackPrefs();
	const info = EMOJI_SETS[emojiSet];
	return (
		<div className="mb-3 flex flex-col gap-2 text-xs" data-testid="emoji-options">
			<div className="flex flex-wrap items-center gap-1">
				{(Object.keys(EMOJI_SETS) as EmojiSetId[]).map((set) => (
					<button
						key={set}
						type="button"
						className={cn(
							"rounded-md border px-2 py-1",
							set === emojiSet ? "bg-accent border-foreground/30" : "text-muted-foreground",
						)}
						onClick={() => {
							setEmojiSet(set);
							refresh();
						}}
					>
						{EMOJI_SETS[set].label}
					</button>
				))}
				<span className="text-muted-foreground ml-2">Skin tone</span>
				{SKIN_TONES.map((tone) => (
					<button
						key={tone.id}
						type="button"
						title={tone.label}
						aria-label={`${tone.label} skin tone`}
						aria-pressed={tone.id === skinTone}
						className={cn("size-5 rounded-full border-2", tone.id === skinTone ? "border-foreground" : "border-transparent")}
						style={{ backgroundColor: tone.swatch }}
						onClick={() => {
							setSkinTone(tone.id);
							refresh();
						}}
					/>
				))}
			</div>
			<p className="text-muted-foreground">
				{info.credit}
				{info.requiresCredit ? ": credit it where your video is published." : "."}
			</p>
		</div>
	);
}

const BRAND_COLOR_LABELS: Record<BrandColorMode, string> = {
	brand: "Brand colour",
	white: "White",
	black: "Black",
};

/** Logo colour, and the trademark notice brand logos need. */
export function BrandPackOptions() {
	const { brandColor, setBrandColor } = useStickerPackPrefs();
	return (
		<div className="mb-3 flex flex-col gap-2 text-xs" data-testid="brand-options">
			<div className="flex flex-wrap items-center gap-1">
				{(Object.keys(BRAND_COLOR_LABELS) as BrandColorMode[]).map((mode) => (
					<button
						key={mode}
						type="button"
						className={cn(
							"rounded-md border px-2 py-1",
							mode === brandColor ? "bg-accent border-foreground/30" : "text-muted-foreground",
						)}
						onClick={() => {
							setBrandColor(mode);
							refresh();
						}}
					>
						{BRAND_COLOR_LABELS[mode]}
					</button>
				))}
			</div>
			<p className="text-muted-foreground rounded-md border border-amber-500/40 bg-amber-500/5 p-2" data-testid="trademark-notice">
				Brand logos are trademarks of their owners. Use them only to refer to the brand, follow the brand's guidelines and do
				not suggest endorsement. Icons from <a className="underline" href="https://simpleicons.org" target="_blank" rel="noreferrer">Simple Icons</a> (CC0).
			</p>
		</div>
	);
}

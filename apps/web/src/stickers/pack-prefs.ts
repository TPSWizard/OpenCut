import { create } from "zustand";
import { persist } from "zustand/middleware";

export type EmojiSetId = "twemoji" | "noto";
export type SkinToneId = "default" | "light" | "medium-light" | "medium" | "medium-dark" | "dark";
/** How brand logos are coloured: the brand's own colour, or plain white or black. */
export type BrandColorMode = "brand" | "white" | "black";

interface StickerPackPrefs {
	emojiSet: EmojiSetId;
	skinTone: SkinToneId;
	brandColor: BrandColorMode;
	setEmojiSet: (emojiSet: EmojiSetId) => void;
	setSkinTone: (skinTone: SkinToneId) => void;
	setBrandColor: (brandColor: BrandColorMode) => void;
}

export const useStickerPackPrefs = create<StickerPackPrefs>()(
	persist(
		(set) => ({
			emojiSet: "twemoji",
			skinTone: "default",
			brandColor: "brand",
			setEmojiSet: (emojiSet) => set({ emojiSet }),
			setSkinTone: (skinTone) => set({ skinTone }),
			setBrandColor: (brandColor) => set({ brandColor }),
		}),
		{ name: "sticker-pack-prefs", version: 1 },
	),
);

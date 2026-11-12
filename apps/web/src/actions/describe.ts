/**
 * Translated text for actions. `ACTIONS` keeps the English descriptions as
 * data; this maps each one to an inline translation literal so the
 * coverage test can see every key. Call it where the text is displayed.
 */

import { t } from "@/i18n";
import { ACTIONS, type TAction, type TActionCategory } from "./definitions";

const DESCRIPTIONS: Record<TAction, () => string> = {
	"toggle-play": () => t("Play/Pause"),
	"stop-playback": () => t("Stop playback"),
	"seek-forward": () => t("Seek forward 1 second"),
	"seek-backward": () => t("Seek backward 1 second"),
	"frame-step-forward": () => t("Frame step forward"),
	"frame-step-backward": () => t("Frame step backward"),
	"jump-forward": () => t("Jump forward 5 seconds"),
	"jump-backward": () => t("Jump backward 5 seconds"),
	"goto-start": () => t("Go to timeline start"),
	"goto-end": () => t("Go to timeline end"),
	split: () => t("Split elements at playhead"),
	"split-left": () => t("Split and remove left"),
	"split-right": () => t("Split and remove right"),
	"delete-selected": () => t("Delete current selection"),
	"copy-selected": () => t("Copy selected elements"),
	"paste-copied": () => t("Paste elements at playhead"),
	"toggle-snapping": () => t("Toggle snapping"),
	"toggle-ripple-editing": () => t("Toggle ripple editing"),
	"toggle-source-audio": () => t("Extract or recover source audio"),
	"select-all": () => t("Select all elements"),
	"group-selected": () => t("Group the selected clips (they select and move together)"),
	"ungroup-selected": () => t("Ungroup the selected clips"),
	"create-compound": () => t("Combine the selected clips into a compound clip"),
	"break-apart-compound": () => t("Break the selected compound clip apart"),
	"select-range": () => t("Select the clips between the in and out marks (unlocked tracks)"),
	"cancel-interaction": () => t("Cancel current interaction"),
	"deselect-all": () => t("Deselect all elements"),
	"duplicate-selected": () => t("Duplicate selected element"),
	"toggle-elements-muted-selected": () => t("Mute/unmute selected elements"),
	"toggle-elements-visibility-selected": () => t("Show/hide selected elements"),
	"toggle-bookmark": () => t("Toggle bookmark at playhead"),
	"mark-in": () => t("Set the in mark at the playhead"),
	"mark-out": () => t("Set the out mark at the playhead"),
	"clear-marks": () => t("Clear the in and out marks"),
	"goto-mark-in": () => t("Go to the in mark"),
	"goto-mark-out": () => t("Go to the out mark"),
	"next-edit": () => t("Go to the next edit point"),
	"previous-edit": () => t("Go to the previous edit point"),
	"ripple-delete": () => t("Ripple delete selection (close the gap)"),
	"delete-marked-range": () => t("Ripple delete between the in and out marks"),
	undo: () => t("Undo"),
	redo: () => t("Redo"),
	"remove-media-asset": () => t("Remove media asset"),
	"remove-media-assets": () => t("Remove media assets"),
};

/** The action's description in the active language. */
export function describeAction(action: TAction): string {
	return DESCRIPTIONS[action]?.() ?? ACTIONS[action].description;
}

const CATEGORIES: Record<TActionCategory, () => string> = {
	playback: () => t("Playback"),
	navigation: () => t("Navigation"),
	editing: () => t("Editing"),
	selection: () => t("Selection"),
	history: () => t("History"),
	timeline: () => t("Timeline"),
	controls: () => t("Controls"),
	assets: () => t("Assets"),
};

/** An action category's label in the active language. */
export function describeActionCategory(category: string): string {
	return CATEGORIES[category as TActionCategory]?.() ?? category;
}

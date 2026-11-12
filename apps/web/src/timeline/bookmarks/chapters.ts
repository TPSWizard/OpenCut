/**
 * Bookmarks written as YouTube chapters: one `MM:SS Title` line each,
 * starting at 00:00 (an "Intro" line is added when the first bookmark is
 * later than a second in). The desktop's `editor::chapters_text` writes
 * the same text.
 */

import type { Bookmark } from "@/timeline/types";

function chapterTime({ seconds, withHours }: { seconds: number; withHours: boolean }): string {
	const total = Math.max(0, Math.floor(seconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const secs = total % 60;
	const pad = (value: number) => value.toString().padStart(2, "0");
	return withHours ? `${hours}:${pad(minutes)}:${pad(secs)}` : `${pad(minutes)}:${pad(secs)}`;
}

export function chaptersText({
	bookmarks,
	ticksPerSecond,
}: {
	bookmarks: Pick<Bookmark, "time" | "note">[];
	ticksPerSecond: number;
}): string {
	const sorted = [...bookmarks].sort((a, b) => a.time - b.time);
	const seconds = (time: number) => time / ticksPerSecond;
	const withHours = sorted.some((bookmark) => seconds(bookmark.time) >= 3600);
	const lines: string[] = [];
	if (sorted.length === 0 || seconds(sorted[0].time) >= 1) {
		lines.push(`${chapterTime({ seconds: 0, withHours })} Intro`);
	}
	sorted.forEach((bookmark, index) => {
		const title = bookmark.note?.trim() || `Chapter ${index + 1}`;
		lines.push(`${chapterTime({ seconds: seconds(bookmark.time), withHours })} ${title}`);
	});
	return lines.join("\n");
}

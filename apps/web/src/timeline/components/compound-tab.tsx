"use client";

import { Button } from "@/components/ui/button";
import { invokeAction } from "@/actions";
import type { CompoundElement } from "@/timeline/types";
import { compoundChildCount } from "@/timeline/compound";
import { useT } from "@/i18n";

/** What a compound clip holds, and the way back to editing its clips. */
export function CompoundTab({ element }: { element: CompoundElement }) {
	const t = useT();
	const clips = compoundChildCount(element);
	const tracks = element.tracks.length;
	const summary =
		clips === 1
			? tracks === 1
				? t("{clips} clip on {tracks} track. Move, trim and split it like one clip; break it apart to edit what is inside.", { clips, tracks })
				: t("{clips} clip on {tracks} tracks. Move, trim and split it like one clip; break it apart to edit what is inside.", { clips, tracks })
			: tracks === 1
				? t("{clips} clips on {tracks} track. Move, trim and split it like one clip; break it apart to edit what is inside.", { clips, tracks })
				: t("{clips} clips on {tracks} tracks. Move, trim and split it like one clip; break it apart to edit what is inside.", { clips, tracks });
	return (
		<div className="flex flex-col gap-3 p-4 text-sm" data-testid="compound-tab">
			<p className="text-muted-foreground">
				{summary}
			</p>
			<div className="flex gap-2">
				<Button size="sm" variant="secondary" onClick={() => invokeAction("break-apart-compound")}>
					{t("Break apart")}
				</Button>
				{element.groupId ? (
					<Button size="sm" variant="outline" onClick={() => invokeAction("ungroup-selected")}>
						{t("Ungroup")}
					</Button>
				) : null}
			</div>
		</div>
	);
}

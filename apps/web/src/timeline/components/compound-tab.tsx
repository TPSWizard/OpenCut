"use client";

import { Button } from "@/components/ui/button";
import { invokeAction } from "@/actions";
import type { CompoundElement } from "@/timeline/types";
import { compoundChildCount } from "@/timeline/compound";

/** What a compound clip holds, and the way back to editing its clips. */
export function CompoundTab({ element }: { element: CompoundElement }) {
	const clips = compoundChildCount(element);
	return (
		<div className="flex flex-col gap-3 p-4 text-sm" data-testid="compound-tab">
			<p className="text-muted-foreground">
				{clips} clip{clips === 1 ? "" : "s"} on {element.tracks.length} track{element.tracks.length === 1 ? "" : "s"}. Move, trim
				and split it like one clip; break it apart to edit what is inside.
			</p>
			<div className="flex gap-2">
				<Button size="sm" variant="secondary" onClick={() => invokeAction("break-apart-compound")}>
					Break apart
				</Button>
				{element.groupId ? (
					<Button size="sm" variant="outline" onClick={() => invokeAction("ungroup-selected")}>
						Ungroup
					</Button>
				) : null}
			</div>
		</div>
	);
}

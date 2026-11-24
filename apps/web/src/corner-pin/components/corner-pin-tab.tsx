"use client";

import { Button } from "@/components/ui/button";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VisualElement } from "@/timeline";
import { resetCornerPinUpdates } from "../apply";
import { CORNER_PIN_ENABLED, CORNER_PIN_KEYS, cornerPinCorners, isConvexPin } from "../pin";

export const CORNER_PIN_TAB_ID = "corner-pin";
export const CORNER_PIN_PARAM_KEYS = [CORNER_PIN_ENABLED, ...CORNER_PIN_KEYS] as const;

/**
 * Corner pin: drag the clip's four corners in the preview (shown while
 * this tab is open) to fit it into a screen, sign or frame in perspective.
 * The corners are keyframable and move with the clip.
 */
export function CornerPinTab({ element, trackId }: { element: VisualElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const params = "params" in element ? element.params : {};
	const enabled = params[CORNER_PIN_ENABLED] === true;
	const drawable = isConvexPin(cornerPinCorners((key) => params[key]));
	return (
		<div data-testid="corner-pin-tab">
			<div className="flex flex-col gap-2 px-3.5 pt-3.5">
				<p className="text-muted-foreground text-xs">
					{t(
						"Drag the four corners in the preview to fit this clip into a screen, a sign or a frame. Corners can be keyframed, and moving or turning the clip carries them.",
					)}
				</p>
				{enabled && !drawable ? (
					<p className="text-destructive text-xs" data-testid="corner-pin-undrawable">
						{t("These corners cross or fold in, so the clip is shown without its pin.")}
					</p>
				) : null}
				<div className="flex flex-wrap gap-1.5">
					<Button
						size="sm"
						variant="outline"
						className="h-8 text-xs"
						data-testid="corner-pin-reset"
						onClick={() =>
							editor.timeline.updateElements({
								updates: [{ trackId, elementId: element.id, patch: resetCornerPinUpdates(element) as never }],
							})
						}
					>
						{t("Reset corners")}
					</Button>
				</div>
			</div>
			<ElementParamsTab element={element} trackId={trackId} paramKeys={CORNER_PIN_PARAM_KEYS} sectionKey="corner-pin" />
		</div>
	);
}

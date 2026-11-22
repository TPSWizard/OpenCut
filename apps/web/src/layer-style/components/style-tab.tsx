"use client";

import { Button } from "@/components/ui/button";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VisualElement } from "@/timeline";
import { SHADOW_PRESETS } from "../style";

export const STYLE_PARAM_KEYS = [
	"cornerRadius",
	"outline.width",
	"outline.color",
	"shadow.enabled",
	"shadow.color",
	"shadow.opacity",
	"shadow.blur",
	"shadow.offsetX",
	"shadow.offsetY",
] as const;

/**
 * Rounded corners, an outline and a drop shadow or glow for a visual clip
 * (see `@/layer-style/style`), with one-click shadow and glow looks.
 */
export function StyleTab({ element, trackId }: { element: VisualElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const params = "params" in element ? element.params : {};
	const apply = (patch: Record<string, string | number | boolean>) =>
		editor.timeline.updateElements({
			updates: [{ trackId, elementId: element.id, patch: { params: { ...params, ...patch } } as never }],
		});
	return (
		<div data-testid="style-tab">
			<div className="flex flex-wrap gap-1.5 px-3.5 pt-3.5">
				<Button size="sm" variant="outline" className="h-8 text-xs" data-testid="style-drop-shadow" onClick={() => apply(SHADOW_PRESETS.shadow)}>
					{t("Drop shadow")}
				</Button>
				<Button size="sm" variant="outline" className="h-8 text-xs" data-testid="style-glow" onClick={() => apply(SHADOW_PRESETS.glow)}>
					{t("Glow")}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					className="h-8 text-xs"
					data-testid="style-clear"
					onClick={() => apply({ cornerRadius: 0, "outline.width": 0, "shadow.enabled": false })}
				>
					{t("Clear style")}
				</Button>
			</div>
			<ElementParamsTab element={element} trackId={trackId} paramKeys={STYLE_PARAM_KEYS} sectionKey="style" />
		</div>
	);
}

import { toast } from "sonner";
import { DraggableItem } from "@/components/editor/panels/assets/draggable-item";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { DEFAULTS } from "@/timeline/defaults";
import { buildTextElement } from "@/timeline/element-utils";
import type { MediaTime } from "@/wasm";
import { getTextTemplateLibrary, insertTextTemplate } from "@/titles/templates";

export function TextView() {
	const t = useT();
	const editor = useEditor();
	const { categories, templates } = getTextTemplateLibrary();

	const handleAddToTimeline = ({ currentTime }: { currentTime: MediaTime }) => {
		const activeScene = editor.scenes.getActiveScene();
		if (!activeScene) return;

		const element = buildTextElement({
			raw: DEFAULTS.text.element,
			startTime: currentTime,
		});

		editor.timeline.insertElement({
			element,
			placement: { mode: "auto" },
		});
	};

	// A template (row 98) goes in at the playhead as grouped text clips.
	const addTemplate = (id: string) => {
		try {
			insertTextTemplate({ editor, id, startTime: editor.playback.getCurrentTime() });
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		}
	};

	return (
		<PanelView title={t("Text")}>
			<DraggableItem
				name={t("Default text")}
				preview={
					<div className="bg-accent flex size-full items-center justify-center rounded">
						<span className="text-xs select-none">{t("Default text")}</span>
					</div>
				}
				dragData={{
					id: "temp-text-id",
					type: DEFAULTS.text.element.type,
					name: DEFAULTS.text.element.name,
					content: "Default text",
				}}
				aspectRatio={1}
				onAddToTimeline={handleAddToTimeline}
				shouldShowLabel={false}
			/>
			<div className="mt-4 flex flex-col gap-4" data-testid="text-templates">
				{categories.map((category) => (
					<section key={category.id} className="flex flex-col gap-1">
						<h3 className="text-muted-foreground text-xs font-semibold">{t(category.label)}</h3>
						{templates
							.filter((template) => template.category === category.id)
							.map((template) => (
								<div key={template.id} className="hover:bg-accent/60 flex items-center gap-2 rounded px-2 py-1.5">
									<div className="flex min-w-0 flex-1 flex-col">
										<span className="text-sm">{t(template.label)}</span>
										<span className="text-muted-foreground text-xs">{t(template.description)}</span>
									</div>
									<Button
										variant="outline"
										size="sm"
										data-testid={`text-template-${template.id}`}
										onClick={() => addTemplate(template.id)}
									>
										{t("Add")}
									</Button>
								</div>
							))}
					</section>
				))}
			</div>
		</PanelView>
	);
}

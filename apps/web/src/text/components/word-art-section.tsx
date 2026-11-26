"use client";

import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { ElementParamsTab } from "@/components/editor/panels/properties/components/element-params-tab";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TextElement } from "@/timeline/types";
import { WORD_ART_KEYS, applyWordArtStyle, getWordArtStyles, wordArtParamsJson } from "../word-art";

/** The word-art params shown as controls (the style is picked from the gallery). */
const WORD_ART_CONTROL_KEYS = WORD_ART_KEYS.filter((key) => key !== "wordArt.style");

/**
 * Word art for a text clip (row 99): a gallery of styles (our own designs)
 * and each part's settings: gradient fill, three outlines, inner shadow.
 */
export function WordArtSection({ element, trackId }: { element: TextElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const current = typeof element.params["wordArt.style"] === "string" ? element.params["wordArt.style"] : "none";
	const active = current !== "none" || wordArtParamsJson(element.params) !== null;

	const pick = (id: string) => {
		try {
			applyWordArtStyle({ editor, targets: [{ trackId, element }], id });
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		}
	};

	return (
		<Section sectionKey={`${element.id}:word-art`} collapsible defaultOpen={active}>
			<SectionHeader>
				<SectionTitle>{t("Word art")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="word-art-section">
					<div className="grid grid-cols-2 gap-1">
						{getWordArtStyles().map((style) => (
							<Button
								key={style.id}
								size="sm"
								variant={current === style.id ? "default" : "outline"}
								title={t(style.description)}
								data-testid={`word-art-${style.id}`}
								onClick={() => pick(style.id)}
							>
								{t(style.label)}
							</Button>
						))}
					</div>
					<p className="text-muted-foreground text-xs">
						{t("Outlines stack outwards from the letters; widths, distance and blur are percent of the font size. Opacity 0 turns the inner shadow off.")}
					</p>
				</div>
				{active && (
					<ElementParamsTab element={element} trackId={trackId} paramKeys={WORD_ART_CONTROL_KEYS} sectionKey="word-art-params" />
				)}
			</SectionContent>
		</Section>
	);
}

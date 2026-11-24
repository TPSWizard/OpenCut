"use client";

import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { TextElement } from "@/timeline/types";
import { applyRichStyle, charIndex, type RichPatch, readRichStyles } from "../rich";

const SWATCHES = ["#ff3b30", "#ffd60a", "#34c759", "#0a84ff", "#ffffff"] as const;

/**
 * Rich text for a text clip: select words in the box, then give just those
 * a colour, weight, slant or size (`@/text/rich`).
 */
export function RichTextSection({ element, trackId }: { element: TextElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const box = useRef<HTMLTextAreaElement>(null);
	const [status, setStatus] = useState<string | null>(null);
	const content = typeof element.params.content === "string" ? element.params.content : "";
	const styled = (readRichStyles(element.params) ?? []).filter((style) => Object.keys(style).length > 0).length;

	const apply = (patch: RichPatch | null) => {
		const field = box.current;
		if (!field) return;
		const start = charIndex(content, field.selectionStart);
		const end = charIndex(content, field.selectionEnd);
		if (start === end) {
			setStatus(t("Select part of the text first."));
			return;
		}
		applyRichStyle({ editor, trackId, element, start, end, patch });
		setStatus(null);
	};

	return (
		<Section sectionKey={`${element.id}:rich-text`} collapsible defaultOpen={styled > 0}>
			<SectionHeader>
				<SectionTitle>{t("Style words")}</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="rich-text-section">
					<p className="text-muted-foreground text-xs">
						{t("Select words below, then pick a colour, weight or size for just those words.")}
					</p>
					<textarea
						ref={box}
						readOnly
						value={content}
						rows={Math.min(4, Math.max(1, content.split("\n").length))}
						className="bg-background w-full resize-none rounded-md border px-2 py-1 text-sm"
						data-testid="rich-text-box"
					/>
					<div className="flex flex-wrap gap-1">
						<Button size="sm" variant="outline" className="font-bold" onClick={() => apply({ bold: true })} data-testid="rich-bold">
							{t("Bold")}
						</Button>
						<Button size="sm" variant="outline" className="italic" onClick={() => apply({ italic: true })} data-testid="rich-italic">
							{t("Italic")}
						</Button>
						<Button size="sm" variant="outline" onClick={() => apply({ scale: 0.75 })} data-testid="rich-smaller">
							{t("Smaller")}
						</Button>
						<Button size="sm" variant="outline" onClick={() => apply({ scale: 1.5 })} data-testid="rich-bigger">
							{t("Bigger")}
						</Button>
						<Button size="sm" variant="outline" onClick={() => apply({ scale: 2 })} data-testid="rich-biggest">
							{t("Biggest")}
						</Button>
					</div>
					<div className="flex flex-wrap items-center gap-1">
						{SWATCHES.map((color) => (
							<button
								key={color}
								type="button"
								aria-label={color}
								className="size-6 rounded border"
								style={{ background: color }}
								onClick={() => apply({ color })}
								data-testid={`rich-color-${color.slice(1)}`}
							/>
						))}
						<input
							type="color"
							aria-label={t("Colour")}
							className="size-6 cursor-pointer rounded border bg-transparent"
							onChange={(event) => apply({ color: event.target.value })}
							data-testid="rich-color-pick"
						/>
						<Button size="sm" variant="ghost" onClick={() => apply(null)} data-testid="rich-clear">
							{t("Clear styles")}
						</Button>
					</div>
					{status && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

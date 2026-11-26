/**
 * Titles (row 98): the text template library (titles, lower thirds,
 * subscribe cards, end credits, a crawl; our own designs) and credit rolls.
 * Both come from the shared `titles` crate through WASM, so a template is
 * placed and a roll scrolls exactly as on the desktop.
 */

import { creditRollPosition, placeTextTemplate, textTemplateLibrary } from "opencut-wasm";
import { BatchCommand } from "@/commands";
import { InsertElementCommand } from "@/commands/timeline/element/insert-element";
import type { EditorCore } from "@/core";
import { buildTextElement } from "@/timeline/element-utils";
import type { CreateTimelineElement } from "@/timeline";
import { mediaTimeFromSeconds, type MediaTime } from "@/wasm";
import { generateUUID } from "@/utils/id";

export interface TextTemplateCategory {
	id: string;
	label: string;
}

export interface TextTemplate {
	id: string;
	label: string;
	category: string;
	description: string;
	duration: number;
	items: Array<{ name: string; content: string }>;
}

export interface PlacedTextItem {
	name: string;
	start: number;
	duration: number;
	params: Record<string, unknown>;
}

let library: { categories: TextTemplateCategory[]; templates: TextTemplate[] } | null = null;

/** The library's categories and templates, in their order. */
export function getTextTemplateLibrary(): { categories: TextTemplateCategory[]; templates: TextTemplate[] } {
	library ??= JSON.parse(textTemplateLibrary());
	return library!;
}

/** The template's text clips placed for a canvas, or null for an unknown id. */
export function placeTemplate({ id, width, height }: { id: string; width: number; height: number }): PlacedTextItem[] | null {
	return JSON.parse(placeTextTemplate(id, Math.round(width), Math.round(height)));
}

/** The template's clips as text elements from `startTime`, grouped when there is more than one. */
export function textTemplateElements({
	id,
	width,
	height,
	startTime,
}: {
	id: string;
	width: number;
	height: number;
	startTime: MediaTime;
}): CreateTimelineElement[] | null {
	const placed = placeTemplate({ id, width, height });
	if (!placed) return null;
	const groupId = placed.length > 1 ? generateUUID() : undefined;
	return placed.map((item) => {
		const element = buildTextElement({
			raw: {
				name: item.name,
				duration: mediaTimeFromSeconds({ seconds: item.duration }),
				params: item.params as never,
			},
			startTime: (startTime + mediaTimeFromSeconds({ seconds: item.start })) as MediaTime,
		});
		return groupId ? ({ ...element, groupId } as CreateTimelineElement) : element;
	});
}

/** Puts a template at `startTime` (one undo step); returns how many clips it added. */
export function insertTextTemplate({ editor, id, startTime }: { editor: EditorCore; id: string; startTime: MediaTime }): number {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const { width, height } = project.settings.canvasSize;
	const elements = textTemplateElements({ id, width, height, startTime });
	if (!elements) throw new Error(`Unknown text template ${id}`);
	const commands = elements.map(
		(element) => new InsertElementCommand({ element, placement: { mode: "auto", trackType: "text" } }),
	);
	editor.command.execute({ command: commands.length === 1 ? commands[0] : new BatchCommand(commands) });
	return elements.length;
}

/** A text clip's roll from its params, or null when it does not roll. */
export function readCreditRoll(params: Record<string, unknown>): { direction: "up" | "left"; speed: number } | null {
	const direction = params.roll;
	if (direction !== "up" && direction !== "left") return null;
	const speed = typeof params.rollSpeed === "number" && Number.isFinite(params.rollSpeed) ? params.rollSpeed : 0;
	return { direction, speed };
}

/**
 * Where a rolling text clip sits along its roll at `localSeconds`: the
 * axis it moves on and its position there (canvas pixels from the centre),
 * from its drawn box relative to its anchor (after scale).
 */
export function creditRollAt({
	roll,
	localSeconds,
	clipSeconds,
	canvasWidth,
	canvasHeight,
	box,
}: {
	roll: { direction: "up" | "left"; speed: number };
	localSeconds: number;
	clipSeconds: number;
	canvasWidth: number;
	canvasHeight: number;
	box: { left: number; top: number; width: number; height: number };
}): { axis: "x" | "y"; position: number } | null {
	const position = creditRollPosition(
		roll.direction,
		roll.speed,
		localSeconds,
		clipSeconds,
		canvasWidth,
		canvasHeight,
		box.left,
		box.top,
		box.width,
		box.height,
	);
	if (!Number.isFinite(position)) return null;
	return { axis: roll.direction === "up" ? "y" : "x", position };
}

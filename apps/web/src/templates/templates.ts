/**
 * Templates are project bundles. The starter ones are ConzaCut's own
 * designs, served from `/templates/` with an `index.json` (built by
 * `cargo run -p editor --example build_templates`; the desktop editor
 * ships the same files). Your own are bundles of your projects saved in
 * the browser: the bundle in OPFS (`templates/`), its details in
 * IndexedDB. Using a template imports its bundle as a new project.
 */

import type { EditorCore } from "@/core";
import { IndexedDBAdapter } from "@/services/storage/indexeddb-adapter";
import { OPFSAdapter } from "@/services/storage/opfs-adapter";
import { generateUUID } from "@/utils/id";

export const TEMPLATES_BASE = "/templates/";
export const TEMPLATES_INDEX_FORMAT = "opencut-templates";

/** How a card draws the template without rendering it. */
export interface TemplatePreview {
	background: string;
	title: string;
	color: string;
	font?: string;
	box?: string;
	italic?: boolean;
	lowerThird?: boolean;
}

export interface TemplateInfo {
	id: string;
	name: string;
	description: string;
	category: string;
	width: number;
	height: number;
	/** Seconds. */
	duration: number;
	fonts: string[];
	preview: TemplatePreview;
	source: "starter" | "mine";
	/** Starter templates: the bundle's file name under `/templates/`. */
	file?: string;
	/** Your templates: when saved, the bundle's size and the project's thumbnail. */
	createdAt?: string;
	bytes?: number;
	thumbnail?: string;
}

type Json = Record<string, unknown>;
const text = (value: unknown, fallback = ""): string => (typeof value === "string" ? value : fallback);
const number = (value: unknown): number => (typeof value === "number" && Number.isFinite(value) ? value : 0);

/** Reads `index.json`, keeping only complete entries with a plain bundle file name. */
export function parseTemplateIndex({ json }: { json: unknown }): TemplateInfo[] {
	const root = (json ?? {}) as Json;
	if (root.format !== TEMPLATES_INDEX_FORMAT || !Array.isArray(root.templates)) return [];
	return root.templates.flatMap((raw): TemplateInfo[] => {
		const entry = (raw ?? {}) as Json;
		const id = text(entry.id);
		const file = text(entry.file);
		const width = number(entry.width);
		const height = number(entry.height);
		if (!/^[a-z0-9-]+$/.test(id) || !/^[a-z0-9-]+\.opencut\.zip$/.test(file) || width <= 0 || height <= 0) return [];
		const preview = (entry.preview ?? {}) as Json;
		return [
			{
				id,
				name: text(entry.name, id),
				description: text(entry.description),
				category: text(entry.category, "Other"),
				width,
				height,
				duration: number(entry.duration),
				fonts: Array.isArray(entry.fonts) ? entry.fonts.filter((font): font is string => typeof font === "string") : [],
				preview: {
					background: text(preview.background, "#000000"),
					title: text(preview.title, text(entry.name, id)),
					color: text(preview.color, "#ffffff"),
					...(typeof preview.font === "string" ? { font: preview.font } : {}),
					...(typeof preview.box === "string" ? { box: preview.box } : {}),
					...(preview.italic === true ? { italic: true } : {}),
					...(preview.lowerThird === true ? { lowerThird: true } : {}),
				},
				source: "starter",
				file,
			},
		];
	});
}

export async function loadStarterTemplates({ fetchImpl = fetch }: { fetchImpl?: typeof fetch } = {}): Promise<TemplateInfo[]> {
	const response = await fetchImpl(`${TEMPLATES_BASE}index.json`);
	if (!response.ok) throw new Error(`Templates could not be loaded (${response.status})`);
	return parseTemplateIndex({ json: await response.json() });
}

const myTemplates = new IndexedDBAdapter<TemplateInfo>({ dbName: "video-editor-templates", storeName: "templates", version: 1 });
const myTemplateFiles = new OPFSAdapter("templates");

export async function listMyTemplates(): Promise<TemplateInfo[]> {
	const all = await myTemplates.getAll();
	return all.sort((a, b) => (b.createdAt ?? "").localeCompare(a.createdAt ?? ""));
}

/** Saves the open project (with its media) as one of your templates. */
export async function saveProjectAsTemplate({
	editor,
	name,
	description = "",
}: {
	editor: EditorCore;
	name: string;
	description?: string;
}): Promise<TemplateInfo> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const { blob } = await editor.project.exportProjectBundle();
	const saved = editor.project.getActive() ?? project;
	const id = generateUUID();
	const { width, height } = saved.settings.canvasSize;
	const background = saved.settings.background;
	const info: TemplateInfo = {
		id,
		name: name.trim() || saved.metadata.name,
		description,
		category: "Yours",
		width,
		height,
		duration: saved.metadata.duration ?? 0,
		fonts: [],
		preview: {
			background: background?.type === "color" ? background.color : "#000000",
			title: name.trim() || saved.metadata.name,
			color: "#ffffff",
		},
		source: "mine",
		createdAt: new Date().toISOString(),
		bytes: blob.size,
		...(saved.metadata.thumbnail ? { thumbnail: saved.metadata.thumbnail } : {}),
	};
	await myTemplateFiles.set({ key: `${id}.opencut.zip`, value: new File([blob], `${id}.opencut.zip`, { type: "application/zip" }) });
	await myTemplates.set({ key: id, value: info });
	return info;
}

export async function deleteMyTemplate({ id }: { id: string }): Promise<void> {
	await myTemplates.remove(id);
	await myTemplateFiles.remove(`${id}.opencut.zip`);
}

async function templateBundle({ template, fetchImpl = fetch }: { template: TemplateInfo; fetchImpl?: typeof fetch }): Promise<Blob> {
	if (template.source === "mine") {
		const file = await myTemplateFiles.get(`${template.id}.opencut.zip`);
		if (!file) throw new Error("This template's file is gone from the browser");
		return file;
	}
	const response = await fetchImpl(`${TEMPLATES_BASE}${template.file}`);
	if (!response.ok) throw new Error(`The template could not be downloaded (${response.status})`);
	return response.blob();
}

/** Makes a new project from a template; returns its id (open it at `/editor/<id>`). */
export async function createProjectFromTemplate({
	editor,
	template,
	name,
}: {
	editor: EditorCore;
	template: TemplateInfo;
	name?: string;
}): Promise<string> {
	const file = await templateBundle({ template });
	return editor.project.importProjectBundle({ file, asNew: { name: name ?? template.name } });
}

/** A template by id among the starter ones and yours. */
export async function findTemplate({ id }: { id: string }): Promise<TemplateInfo | undefined> {
	const mine = await myTemplates.get(id);
	if (mine) return mine;
	return (await loadStarterTemplates()).find((template) => template.id === id);
}

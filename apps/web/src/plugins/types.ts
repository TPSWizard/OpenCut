/**
 * Plugin model. A plugin is described by a JSON manifest served from any URL.
 *
 * ```json
 * {
 *   "id": "com.example.timeline-stats",
 *   "name": "Timeline stats",
 *   "version": "1.0.0",
 *   "description": "Shows clip counts and lets you add a title card.",
 *   "panel": { "title": "Stats", "url": "./panel.html" },
 *   "entry": "./worker.js",
 *   "apiVersion": "1"
 * }
 * ```
 *
 * - `panel` adds a tab to the assets panel that hosts the page in a sandboxed
 *   iframe. The page talks to the editor through the Editor API message
 *   bridge (see `public/plugin-sdk/opencut-plugin.js`).
 * - `entry` is an ES module run in a dedicated Worker for the lifetime of the
 *   open project. Its default export (or `activate`) receives the remote
 *   Editor API client.
 *
 * Relative URLs resolve against the manifest URL.
 */
export interface PluginManifest {
	id: string;
	name: string;
	version: string;
	description?: string;
	author?: string;
	homepage?: string;
	/** Major Editor API version the plugin was written for. */
	apiVersion?: string;
	panel?: {
		title: string;
		url: string;
	};
	entry?: string;
}

export interface InstalledPlugin {
	/** Where the manifest was fetched from. */
	manifestUrl: string;
	manifest: PluginManifest;
	/** `panel.url` and `entry` resolved to absolute URLs. */
	panelUrl?: string;
	entryUrl?: string;
	enabled: boolean;
	installedAt: string;
}

export type PluginStatus =
	| { state: "idle" }
	| { state: "running" }
	| { state: "error"; message: string };

const ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,127}$/i;

export class PluginManifestError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "PluginManifestError";
	}
}

function requireString(value: unknown, field: string): string {
	if (typeof value !== "string" || value.trim().length === 0) {
		throw new PluginManifestError(`manifest.${field} must be a non-empty string`);
	}
	return value.trim();
}

export function parsePluginManifest(value: unknown): PluginManifest {
	if (typeof value !== "object" || value === null) {
		throw new PluginManifestError("manifest must be a JSON object");
	}
	const raw = value as Record<string, unknown>;
	const id = requireString(raw.id, "id");
	if (!ID_PATTERN.test(id)) {
		throw new PluginManifestError(
			"manifest.id may only contain letters, digits, dots, dashes and underscores",
		);
	}
	const manifest: PluginManifest = {
		id,
		name: requireString(raw.name, "name"),
		version: requireString(raw.version, "version"),
	};
	if (raw.description !== undefined) {
		manifest.description = requireString(raw.description, "description");
	}
	if (raw.author !== undefined) manifest.author = requireString(raw.author, "author");
	if (raw.homepage !== undefined) {
		manifest.homepage = requireString(raw.homepage, "homepage");
	}
	if (raw.apiVersion !== undefined) {
		manifest.apiVersion = requireString(raw.apiVersion, "apiVersion");
	}
	if (raw.panel !== undefined) {
		if (typeof raw.panel !== "object" || raw.panel === null) {
			throw new PluginManifestError("manifest.panel must be an object");
		}
		const panel = raw.panel as Record<string, unknown>;
		manifest.panel = {
			title: requireString(panel.title, "panel.title"),
			url: requireString(panel.url, "panel.url"),
		};
	}
	if (raw.entry !== undefined) manifest.entry = requireString(raw.entry, "entry");
	if (!manifest.panel && !manifest.entry) {
		throw new PluginManifestError(
			"manifest needs a panel, an entry module, or both",
		);
	}
	return manifest;
}

export function resolvePluginUrl({
	base,
	url,
}: {
	base: string;
	url: string;
}): string {
	return new URL(url, base).toString();
}

export function pluginTabKey(pluginId: string): `plugin:${string}` {
	return `plugin:${pluginId}`;
}

export function isPluginTabKey(value: string): value is `plugin:${string}` {
	return value.startsWith("plugin:");
}

export function pluginIdFromTabKey(value: `plugin:${string}`): string {
	return value.slice("plugin:".length);
}

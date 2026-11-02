import { parse as parseYaml } from "yaml";

export type Change = { type: string; text: string };

export type Release = {
	version: string;
	date: string;
	published: boolean;
	title: string;
	description: string;
	summary?: string;
	changes: Change[];
};

type ChangeSectionConfig = {
	title: string;
	order: number;
	collapsible?: boolean;
};

const knownSectionConfigs: Record<string, ChangeSectionConfig> = {
	new: { title: "Features", order: 0 },
	improved: { title: "Improvements", order: 1 },
	fixed: { title: "Fixes", order: 2 },
	breaking: { title: "Breaking Changes", order: 3 },
	technical: {
		title: "Technical details",
		order: 4,
		collapsible: true,
	},
};

const FRONTMATTER_PATTERN = /^---\r?\n([\s\S]*?)\r?\n---/;

// The release notes live at the repository root (`changelog/*.md`) so they are
// shared by every app. Vite inlines them at build time.
const changelogSources = import.meta.glob("../../../../changelog/*.md", {
	query: "?raw",
	import: "default",
	eager: true,
}) as Record<string, string>;

function isChange(value: unknown): value is Change {
	return (
		typeof value === "object" &&
		value !== null &&
		typeof (value as Change).type === "string" &&
		typeof (value as Change).text === "string"
	);
}

export function parseReleaseMarkdown({
	source,
	fileName,
}: {
	source: string;
	fileName: string;
}): Release | null {
	const match = FRONTMATTER_PATTERN.exec(source);
	if (!match) return null;

	const data = parseYaml(match[1]) as Record<string, unknown> | null;
	if (!data || typeof data !== "object") return null;

	const version =
		typeof data.version === "string"
			? data.version
			: fileName.replace(/\.md$/, "");
	const changes = Array.isArray(data.changes)
		? data.changes.filter(isChange)
		: [];

	return {
		version,
		date: typeof data.date === "string" ? data.date : String(data.date ?? ""),
		published: data.published !== false,
		title: typeof data.title === "string" ? data.title : `v${version}`,
		description:
			typeof data.description === "string" ? data.description : "",
		summary: typeof data.summary === "string" ? data.summary : undefined,
		changes,
	};
}

function loadReleases(): Release[] {
	const releases: Release[] = [];
	for (const [path, source] of Object.entries(changelogSources)) {
		const fileName = path.split("/").pop() ?? path;
		const release = parseReleaseMarkdown({ source, fileName });
		if (release) releases.push(release);
	}
	return releases;
}

export const allChangelogs: Release[] = loadReleases();

function getSectionConfig({ type }: { type: string }): ChangeSectionConfig {
	return (
		knownSectionConfigs[type] ?? {
			title: type.charAt(0).toUpperCase() + type.slice(1),
			order: Number.MAX_SAFE_INTEGER,
		}
	);
}

export function getSectionTitle({ type }: { type: string }): string {
	return getSectionConfig({ type }).title;
}

export function isSectionCollapsible({ type }: { type: string }): boolean {
	return getSectionConfig({ type }).collapsible ?? false;
}

export function groupAndOrderChanges({ changes }: { changes: Change[] }) {
	const typeEncounterOrder = new Map<string, number>();
	const grouped = changes.reduce<Record<string, Change[]>>((acc, change) => {
		if (!typeEncounterOrder.has(change.type)) {
			typeEncounterOrder.set(change.type, typeEncounterOrder.size);
		}
		if (!acc[change.type]) acc[change.type] = [];
		acc[change.type].push(change);
		return acc;
	}, {});

	const orderedTypes = Object.keys(grouped).sort((left, right) => {
		const orderDifference =
			getSectionConfig({ type: left }).order -
			getSectionConfig({ type: right }).order;
		if (orderDifference !== 0) {
			return orderDifference;
		}
		return (
			(typeEncounterOrder.get(left) ?? Number.MAX_SAFE_INTEGER) -
			(typeEncounterOrder.get(right) ?? Number.MAX_SAFE_INTEGER)
		);
	});

	return { grouped, orderedTypes };
}

function isPublishedRelease({ published }: Release) {
	return published;
}

export function getSortedReleases() {
	return allChangelogs
		.filter(isPublishedRelease)
		.sort((a, b) =>
			b.version.localeCompare(a.version, undefined, { numeric: true }),
		);
}

export function getReleaseByVersion({ version }: { version: string }) {
	return getSortedReleases().find((release) => release.version === version);
}

/**
 * Project bundles: a zip holding the project (as it is stored) and the
 * media files it uses. The native desktop editor reads and writes the same
 * format, so a bundle is how a project moves between the two.
 *
 *   project.json      {"format": "opencut-project-bundle", "formatVersion": 1,
 *                      "project": SerializedProject,
 *                      "media": [MediaAssetData + {"file": "media/<id>.<ext>"}]}
 *   media/<id>.<ext>  the files
 */
import { strFromU8, strToU8, unzip, zip, type Zippable } from "fflate";
import type { MediaAsset } from "@/media/types";
import type { TProject } from "@/project/types";
import type {
	MediaAssetData,
	SerializedProject,
} from "@/services/storage/types";
import { getProjectDurationFromScenes } from "@/timeline/scenes";

export const PROJECT_BUNDLE_FORMAT = "opencut-project-bundle";
export const PROJECT_BUNDLE_FORMAT_VERSION = 1;
export const PROJECT_BUNDLE_EXTENSION = ".opencut.zip";
const PROJECT_ENTRY = "project.json";

export type BundledMediaData = MediaAssetData & {
	/** Entry name inside the zip. */
	file: string;
};

export interface ProjectBundleManifest {
	format: typeof PROJECT_BUNDLE_FORMAT;
	formatVersion: number;
	project: SerializedProject;
	media: BundledMediaData[];
}

export interface ReadProjectBundle {
	project: TProject;
	media: Array<{ data: MediaAssetData; file: File }>;
}

const MIME_BY_EXTENSION: Record<string, string> = {
	".mp4": "video/mp4",
	".m4v": "video/mp4",
	".mov": "video/quicktime",
	".webm": "video/webm",
	".mkv": "video/x-matroska",
	".mp3": "audio/mpeg",
	".wav": "audio/wav",
	".m4a": "audio/mp4",
	".aac": "audio/aac",
	".ogg": "audio/ogg",
	".flac": "audio/flac",
	".png": "image/png",
	".jpg": "image/jpeg",
	".jpeg": "image/jpeg",
	".gif": "image/gif",
	".webp": "image/webp",
	".bmp": "image/bmp",
	".svg": "image/svg+xml",
};

function extensionOf({ name }: { name: string }): string {
	const index = name.lastIndexOf(".");
	return index > 0 ? name.slice(index).toLowerCase() : "";
}

function mimeFor({ name, fallback }: { name: string; fallback: string }): string {
	return MIME_BY_EXTENSION[extensionOf({ name })] ?? fallback;
}

/** The project as the storage service writes it (dates as ISO strings). */
export function serializeProject({
	project,
}: {
	project: TProject;
}): SerializedProject {
	return {
		metadata: {
			id: project.metadata.id,
			name: project.metadata.name,
			thumbnail: project.metadata.thumbnail,
			duration:
				project.metadata.duration ??
				getProjectDurationFromScenes({ scenes: project.scenes }),
			createdAt: project.metadata.createdAt.toISOString(),
			updatedAt: project.metadata.updatedAt.toISOString(),
		},
		scenes: project.scenes.map((scene) => ({
			id: scene.id,
			name: scene.name,
			isMain: scene.isMain,
			tracks: scene.tracks,
			bookmarks: scene.bookmarks,
			createdAt: scene.createdAt.toISOString(),
			updatedAt: scene.updatedAt.toISOString(),
		})),
		currentSceneId: project.currentSceneId,
		settings: project.settings,
		version: project.version,
		timelineViewState: project.timelineViewState,
	};
}

export function deserializeProject({
	serialized,
}: {
	serialized: SerializedProject;
}): TProject {
	const scenes = serialized.scenes.map((scene) => ({
		id: scene.id,
		name: scene.name,
		isMain: scene.isMain,
		tracks: scene.tracks,
		bookmarks: scene.bookmarks ?? [],
		createdAt: new Date(scene.createdAt),
		updatedAt: new Date(scene.updatedAt),
	}));
	return {
		metadata: {
			id: serialized.metadata.id,
			name: serialized.metadata.name,
			thumbnail: serialized.metadata.thumbnail,
			duration:
				serialized.metadata.duration ??
				getProjectDurationFromScenes({ scenes }),
			createdAt: new Date(serialized.metadata.createdAt),
			updatedAt: new Date(serialized.metadata.updatedAt),
		},
		scenes,
		currentSceneId: serialized.currentSceneId || scenes[0]?.id || "",
		settings: serialized.settings,
		version: serialized.version,
		timelineViewState: serialized.timelineViewState,
	};
}

function zipAsync(files: Zippable): Promise<Uint8Array> {
	return new Promise((resolve, reject) => {
		zip(files, (error, data) => (error ? reject(error) : resolve(data)));
	});
}

function unzipAsync(data: Uint8Array): Promise<Record<string, Uint8Array>> {
	return new Promise((resolve, reject) => {
		unzip(data, (error, files) => (error ? reject(error) : resolve(files)));
	});
}

/** A file name the bundle's media entry can use for a media asset. */
export function bundleMediaEntryName({
	asset,
}: {
	asset: Pick<MediaAsset, "id" | "name" | "file">;
}): string {
	const extension =
		extensionOf({ name: asset.name }) || extensionOf({ name: asset.file.name });
	return `media/${asset.id}${extension}`;
}

/**
 * Packs a project and its media into a bundle. Media is stored without
 * compression (it already is compressed); the project JSON is deflated.
 */
export async function buildProjectBundle({
	project,
	mediaAssets,
}: {
	project: TProject;
	mediaAssets: MediaAsset[];
}): Promise<Blob> {
	const files: Zippable = {};
	const media: BundledMediaData[] = [];
	for (const asset of mediaAssets) {
		const entry = bundleMediaEntryName({ asset });
		files[entry] = [new Uint8Array(await asset.file.arrayBuffer()), { level: 0 }];
		media.push({
			id: asset.id,
			name: asset.name,
			type: asset.type,
			size: asset.file.size,
			lastModified: asset.file.lastModified,
			width: asset.width,
			height: asset.height,
			duration: asset.duration,
			fps: asset.fps,
			hasAudio: asset.hasAudio,
			file: entry,
		});
	}
	const manifest: ProjectBundleManifest = {
		format: PROJECT_BUNDLE_FORMAT,
		formatVersion: PROJECT_BUNDLE_FORMAT_VERSION,
		project: serializeProject({ project }),
		media,
	};
	files[PROJECT_ENTRY] = [
		strToU8(JSON.stringify(manifest, null, 2)),
		{ level: 6 },
	];
	const bytes = await zipAsync(files);
	return new Blob([bytes as BlobPart], { type: "application/zip" });
}

export class ProjectBundleError extends Error {}

/** Unpacks a bundle into a project (with dates) and its media as Files. */
export async function readProjectBundle({
	file,
}: {
	file: Blob;
}): Promise<ReadProjectBundle> {
	let entries: Record<string, Uint8Array>;
	try {
		entries = await unzipAsync(new Uint8Array(await file.arrayBuffer()));
	} catch (error) {
		throw new ProjectBundleError(
			`Not a zip file: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
	const manifestBytes = entries[PROJECT_ENTRY];
	if (!manifestBytes) {
		throw new ProjectBundleError(
			`Not an OpenCut project bundle (no ${PROJECT_ENTRY})`,
		);
	}
	let manifest: ProjectBundleManifest;
	try {
		manifest = JSON.parse(strFromU8(manifestBytes)) as ProjectBundleManifest;
	} catch (error) {
		throw new ProjectBundleError(
			`${PROJECT_ENTRY} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
	if (manifest.format !== PROJECT_BUNDLE_FORMAT) {
		throw new ProjectBundleError(
			`Unknown bundle format ${JSON.stringify(manifest.format)}`,
		);
	}
	if (
		typeof manifest.formatVersion !== "number" ||
		manifest.formatVersion > PROJECT_BUNDLE_FORMAT_VERSION
	) {
		throw new ProjectBundleError(
			`Bundle format version ${String(manifest.formatVersion)} is newer than this app understands (${PROJECT_BUNDLE_FORMAT_VERSION})`,
		);
	}
	const project = manifest.project;
	if (
		!project ||
		typeof project !== "object" ||
		!project.metadata ||
		typeof project.metadata.id !== "string" ||
		typeof project.metadata.name !== "string" ||
		!Array.isArray(project.scenes) ||
		!project.settings ||
		typeof project.settings !== "object" ||
		typeof project.version !== "number"
	) {
		throw new ProjectBundleError(
			"The bundle's project is incomplete (metadata, scenes, settings and version are required)",
		);
	}
	if (manifest.media !== undefined && !Array.isArray(manifest.media)) {
		throw new ProjectBundleError("The bundle's media list is not a list");
	}
	const media: ReadProjectBundle["media"] = [];
	const seen = new Set<string>();
	for (const item of manifest.media ?? []) {
		if (
			!item ||
			typeof item.id !== "string" ||
			typeof item.name !== "string" ||
			typeof item.file !== "string" ||
			(item.type !== "video" && item.type !== "image" && item.type !== "audio")
		) {
			throw new ProjectBundleError(
				"A media entry in the bundle is incomplete (id, name, type and file are required)",
			);
		}
		if (seen.has(item.id)) {
			throw new ProjectBundleError(`Media id ${item.id} appears twice in the bundle`);
		}
		seen.add(item.id);
		const bytes = entries[item.file];
		if (!bytes) {
			throw new ProjectBundleError(`The bundle is missing ${item.file}`);
		}
		if (typeof item.size === "number" && item.size !== bytes.byteLength) {
			throw new ProjectBundleError(
				`${item.file} is ${bytes.byteLength} bytes but the manifest says ${item.size}`,
			);
		}
		const { file: _entry, ...data } = item;
		media.push({
			data,
			file: new File([bytes as BlobPart], item.name, {
				type: mimeFor({ name: item.name, fallback: "" }),
				lastModified: item.lastModified,
			}),
		});
	}
	return {
		project: deserializeProject({ serialized: manifest.project }),
		media,
	};
}

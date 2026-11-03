import { describe, expect, test } from "vitest";
import {
	PROJECT_BUNDLE_FORMAT,
	ProjectBundleError,
	buildProjectBundle,
	readProjectBundle,
	serializeProject,
} from "@/project/bundle";
import type { TProject } from "@/project/types";
import { buildDefaultScene } from "@/timeline/scenes";
import type { MediaAsset } from "@/media/types";
import { strToU8, zipSync } from "fflate";
import { mediaTime } from "@/wasm";

function sampleProject(): TProject {
	const scene = buildDefaultScene({ name: "Main scene", isMain: true });
	scene.tracks.main.elements.push({
		id: "el-1",
		name: "clip",
		type: "video",
		mediaId: "m-1",
		startTime: mediaTime({ ticks: 0 }),
		duration: mediaTime({ ticks: 240_000 }),
		trimStart: mediaTime({ ticks: 0 }),
		trimEnd: mediaTime({ ticks: 0 }),
		params: { opacity: 0.5 },
		masks: [],
	});
	scene.tracks.overlay.push({
		id: "t-text",
		name: "Text",
		type: "text",
		hidden: false,
		elements: [
			{
				id: "el-2",
				name: "Title",
				type: "text",
				startTime: mediaTime({ ticks: 120_000 }),
				duration: mediaTime({ ticks: 240_000 }),
				trimStart: mediaTime({ ticks: 0 }),
				trimEnd: mediaTime({ ticks: 0 }),
				params: { content: "Hello", fontSize: 15 },
				animations: {
					opacity: {
						keys: [
							{
								id: "k1",
								time: mediaTime({ ticks: 0 }),
								value: 0,
								segmentToNext: "linear",
								tangentMode: "flat",
							},
						],
					},
				},
			},
		],
	});
	return {
		metadata: {
			id: "p-1",
			name: "Bundle me",
			duration: mediaTime({ ticks: 360_000 }),
			createdAt: new Date("2024-05-06T12:53:20.123Z"),
			updatedAt: new Date("2024-05-07T00:00:00.000Z"),
		},
		scenes: [scene],
		currentSceneId: scene.id,
		settings: {
			fps: { numerator: 30, denominator: 1 },
			canvasSize: { width: 640, height: 360 },
			canvasSizeMode: "preset",
			lastCustomCanvasSize: null,
			originalCanvasSize: null,
			background: { type: "blur", blurIntensity: 200 },
		},
		version: 31,
	};
}

function sampleAsset(): MediaAsset {
	return {
		id: "m-1",
		name: "clip.mp4",
		type: "video",
		width: 320,
		height: 180,
		duration: mediaTime({ ticks: 2 }),
		fps: 30,
		hasAudio: true,
		file: new File([new Uint8Array([1, 2, 3, 4, 5])], "clip.mp4", {
			type: "video/mp4",
			lastModified: 1_700_000_000_000,
		}),
	};
}

describe("project bundles", () => {
	test("packs the project and media and reads them back", async () => {
		const project = sampleProject();
		const blob = await buildProjectBundle({
			project,
			mediaAssets: [sampleAsset()],
		});
		expect(blob.type).toBe("application/zip");

		const read = await readProjectBundle({ file: blob });
		expect(serializeProject({ project: read.project })).toEqual(
			serializeProject({ project }),
		);
		expect(read.project.metadata.createdAt).toBeInstanceOf(Date);
		expect(read.project.scenes[0].createdAt).toBeInstanceOf(Date);
		expect(read.media).toHaveLength(1);
		const [{ data, file }] = read.media;
		expect(data).toMatchObject({
			id: "m-1",
			name: "clip.mp4",
			type: "video",
			width: 320,
			duration: mediaTime({ ticks: 2 }),
			hasAudio: true,
			size: 5,
			lastModified: 1_700_000_000_000,
		});
		expect("file" in data).toBe(false);
		expect(file.name).toBe("clip.mp4");
		expect(file.type).toBe("video/mp4");
		expect(Array.from(new Uint8Array(await file.arrayBuffer()))).toEqual([
			1, 2, 3, 4, 5,
		]);
	});

	test("the manifest names the format and lists media entries", async () => {
		const blob = await buildProjectBundle({
			project: sampleProject(),
			mediaAssets: [sampleAsset()],
		});
		const { unzipSync, strFromU8 } = await import("fflate");
		const entries = unzipSync(new Uint8Array(await blob.arrayBuffer()));
		expect(Object.keys(entries).sort()).toEqual([
			"media/m-1.mp4",
			"project.json",
		]);
		const manifest = JSON.parse(strFromU8(entries["project.json"]));
		expect(manifest.format).toBe(PROJECT_BUNDLE_FORMAT);
		expect(manifest.formatVersion).toBe(1);
		expect(manifest.media[0].file).toBe("media/m-1.mp4");
		expect(manifest.project.metadata.createdAt).toBe(
			"2024-05-06T12:53:20.123Z",
		);
	});

	test("rejects files that are not bundles", async () => {
		await expect(
			readProjectBundle({ file: new Blob([new Uint8Array([1, 2, 3])]) }),
		).rejects.toBeInstanceOf(ProjectBundleError);

		const noManifest = zipSync({ "readme.txt": strToU8("hi") });
		await expect(
			readProjectBundle({ file: new Blob([noManifest as BlobPart]) }),
		).rejects.toThrow(/not an OpenCut project bundle/i);

		const wrongFormat = zipSync({
			"project.json": strToU8(
				JSON.stringify({ format: "something-else", formatVersion: 1 }),
			),
		});
		await expect(
			readProjectBundle({ file: new Blob([wrongFormat as BlobPart]) }),
		).rejects.toThrow(/unknown bundle format/i);

		const tooNew = zipSync({
			"project.json": strToU8(
				JSON.stringify({ format: PROJECT_BUNDLE_FORMAT, formatVersion: 99 }),
			),
		});
		await expect(
			readProjectBundle({ file: new Blob([tooNew as BlobPart]) }),
		).rejects.toThrow(/newer/i);

		const noSettings = zipSync({
			"project.json": strToU8(
				JSON.stringify({
					format: PROJECT_BUNDLE_FORMAT,
					formatVersion: 1,
					project: { metadata: { id: "p", name: "p" }, scenes: [], version: 31 },
					media: [],
				}),
			),
		});
		await expect(
			readProjectBundle({ file: new Blob([noSettings as BlobPart]) }),
		).rejects.toThrow(/incomplete/);

		const badMedia = zipSync({
			"project.json": strToU8(
				JSON.stringify({
					format: PROJECT_BUNDLE_FORMAT,
					formatVersion: 1,
					project: serializeProject({ project: sampleProject() }),
					media: [{ id: "m-1", type: "video", file: "media/m-1.mp4" }],
				}),
			),
			"media/m-1.mp4": new Uint8Array([1]),
		});
		await expect(
			readProjectBundle({ file: new Blob([badMedia as BlobPart]) }),
		).rejects.toThrow(/media entry/);

		const missingMedia = zipSync({
			"project.json": strToU8(
				JSON.stringify({
					format: PROJECT_BUNDLE_FORMAT,
					formatVersion: 1,
					project: serializeProject({ project: sampleProject() }),
					media: [{ id: "m-1", name: "clip.mp4", type: "video", file: "media/m-1.mp4" }],
				}),
			),
		});
		await expect(
			readProjectBundle({ file: new Blob([missingMedia as BlobPart]) }),
		).rejects.toThrow(/missing media\/m-1.mp4/);
	});
});

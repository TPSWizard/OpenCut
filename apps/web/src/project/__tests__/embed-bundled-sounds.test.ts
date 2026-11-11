import { describe, expect, it } from "vitest";
import { embedBundledSounds } from "../bundle";

function serializedWith(elements: Array<Record<string, unknown>>) {
	return {
		metadata: {
			id: "p",
			name: "P",
			thumbnail: "",
			duration: 0,
			createdAt: "2026-01-01T00:00:00.000Z",
			updatedAt: "2026-01-01T00:00:00.000Z",
		},
		scenes: [
			{
				id: "s",
				name: "Main",
				isMain: true,
				tracks: { overlay: [], main: { id: "m", type: "video", elements: [] }, audio: [{ id: "a", type: "audio", elements }] },
				bookmarks: [],
				createdAt: "2026-01-01T00:00:00.000Z",
				updatedAt: "2026-01-01T00:00:00.000Z",
			},
		],
		currentSceneId: "s",
		settings: { fps: { numerator: 30, denominator: 1 }, canvasSize: { width: 1920, height: 1080 } },
	} as unknown as Parameters<typeof embedBundledSounds>[0]["serialized"];
}

describe("embedBundledSounds", () => {
	it("copies bundled sounds into the bundle and rewrites their elements as uploads", async () => {
		const serialized = serializedWith([
			{ id: "e1", type: "audio", sourceType: "library", sourceUrl: "/sounds/kenney-ui/click-1.ogg", name: "click 1", duration: 14_400, sourceDuration: 14_400, startTime: 0 },
			{ id: "e2", type: "audio", sourceType: "library", sourceUrl: "/sounds/kenney-ui/click-1.ogg", name: "click 1", duration: 14_400, startTime: 50_000 },
			{ id: "e3", type: "audio", sourceType: "library", sourceUrl: "https://freesound.org/x.mp3", name: "remote", duration: 1, startTime: 0 },
			{ id: "e4", type: "audio", sourceType: "upload", mediaId: "m1", name: "mine", duration: 1, startTime: 0 },
		]);
		const fetched: string[] = [];
		const fetchImpl = (async (url: string) => {
			fetched.push(url);
			return new Response(new Uint8Array([1, 2, 3]), { status: 200 });
		}) as unknown as typeof fetch;
		const added = await embedBundledSounds({ serialized, fetchImpl });
		expect(fetched).toEqual(["/sounds/kenney-ui/click-1.ogg"]);
		expect(added).toHaveLength(1);
		expect(added[0].data).toMatchObject({
			id: "bundled-sound-kenney-ui-click-1-ogg",
			type: "audio",
			hasAudio: true,
			size: 3,
			file: "media/bundled-sound-kenney-ui-click-1-ogg.ogg",
		});
		expect(added[0].data.duration).toBeCloseTo(0.12, 6);
		const elements = (serialized.scenes[0].tracks as unknown as { audio: Array<{ elements: Array<Record<string, unknown>> }> }).audio[0].elements;
		expect(elements[0]).toMatchObject({ id: "e1", sourceType: "upload", mediaId: "bundled-sound-kenney-ui-click-1-ogg" });
		expect(elements[0]).not.toHaveProperty("sourceUrl");
		expect(elements[1]).toMatchObject({ id: "e2", sourceType: "upload", mediaId: "bundled-sound-kenney-ui-click-1-ogg" });
		expect(elements[2]).toMatchObject({ id: "e3", sourceType: "library", sourceUrl: "https://freesound.org/x.mp3" });
		expect(elements[3]).toMatchObject({ id: "e4", sourceType: "upload", mediaId: "m1" });
	});

	it("leaves an element alone when its file cannot be fetched", async () => {
		const serialized = serializedWith([
			{ id: "e1", type: "audio", sourceType: "library", sourceUrl: "/sounds/missing.ogg", name: "gone", duration: 1, startTime: 0 },
		]);
		const fetchImpl = (async () => new Response(null, { status: 404 })) as unknown as typeof fetch;
		expect(await embedBundledSounds({ serialized, fetchImpl })).toEqual([]);
		const elements = (serialized.scenes[0].tracks as unknown as { audio: Array<{ elements: Array<Record<string, unknown>> }> }).audio[0].elements;
		expect(elements[0]).toMatchObject({ sourceType: "library" });
	});
});

import { describe, expect, test } from "bun:test";
import {
	cacheKeyFor,
	normalizePixabayImage,
	normalizePixabayVideo,
	pixabayUrl,
	pixabayVideoFile,
	type PixabayImageHit,
	type PixabayVideoHit,
} from "./stock";

// Shapes as in Pixabay's API documentation (https://pixabay.com/api/docs/).
const image: PixabayImageHit = {
	id: 195893,
	pageURL: "https://pixabay.com/en/blossom-bloom-flower-195893/",
	tags: "blossom, bloom, flower",
	previewURL: "https://cdn.pixabay.com/photo/2013/10/15/09/12/flower-195893_150.jpg",
	webformatURL: "https://pixabay.com/get/35bbf209e13e39d2_640.jpg",
	largeImageURL: "https://pixabay.com/get/ed6a99fd0a76647_1280.jpg",
	imageWidth: 4000,
	imageHeight: 2250,
	user: "Josch13",
	user_id: 48777,
};

const video: PixabayVideoHit = {
	id: 125,
	pageURL: "https://pixabay.com/videos/id-125/",
	tags: "flowers, yellow, blossom",
	duration: 12,
	videos: {
		large: { url: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_large.mp4", width: 1920, height: 1080, size: 6615235, thumbnail: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_large.jpg" },
		medium: { url: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_medium.mp4", width: 1280, height: 720, size: 3562083, thumbnail: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_medium.jpg" },
		small: { url: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_small.mp4", width: 640, height: 360, size: 1030736, thumbnail: "https://cdn.pixabay.com/video/2015/08/08/125-135736646_small.jpg" },
	},
	user: "CoverrFootage",
	user_id: 1281706,
};

describe("stock", () => {
	test("Pixabay images become stock items with credit", () => {
		const item = normalizePixabayImage(image);
		expect(item).toMatchObject({ provider: "pixabay", id: "195893", kind: "image", title: "Blossom", width: 4000, creator: "Josch13" });
		expect(item.creatorUrl).toBe("https://pixabay.com/users/Josch13-48777/");
		expect(item.attribution).toContain("Pixabay Content License");
	});

	test("Pixabay videos pick the 1080p file and a small thumbnail", () => {
		const item = normalizePixabayVideo(video);
		expect(item).toMatchObject({ kind: "video", width: 1920, height: 1080, duration: 12, title: "Flowers" });
		expect(item.thumbnail).toContain("_small.jpg");
		expect(pixabayVideoFile({ ...video, videos: { small: video.videos.small } })?.width).toBe(640);
	});

	test("the cache key leaves the API key out", () => {
		const url = pixabayUrl({ key: "secret", kind: "video", q: "sea", page: 2 });
		expect(url).toContain("/api/videos/?");
		expect(url).toContain("key=secret");
		const key = cacheKeyFor(url);
		expect(key).not.toContain("secret");
		expect(key).toContain("q=sea");
		expect(pixabayUrl({ key: "k", kind: "image", id: "195893" })).toContain("id=195893");
	});
});

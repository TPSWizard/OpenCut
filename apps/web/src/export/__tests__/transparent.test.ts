import { describe, expect, it } from "vitest";
import { TRANSPARENT_BACKGROUND, TRANSPARENT_VIDEO, kindTakesTransparency } from "../transparent";
import { EXPORT_KIND_VALUES } from "../kinds";

describe("transparent export (row 97)", () => {
	it("is offered for videos, stills and image sequences only", () => {
		expect(EXPORT_KIND_VALUES.filter(kindTakesTransparency)).toEqual(["video", "still", "frames"]);
	});

	it("draws nothing behind the clips and writes VP9 in WebM", () => {
		// `buildScene` adds no colour node for a "transparent" colour background.
		expect(TRANSPARENT_BACKGROUND).toEqual({ type: "color", color: "transparent" });
		expect(TRANSPARENT_VIDEO).toEqual({ format: "webm", codec: "vp9" });
	});
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { faceTrackFacesAt, faceTrackMedia, retouchFaceUniforms } from "opencut-wasm";
import { withFaceOvals } from "../track";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/face-track/cases.json"), "utf8")) as {
	track: string;
	lookups: { time: number; faces: number[][] }[];
	invalid: string[];
	ovals: {
		name: string;
		placement: { centerX: number; centerY: number; width: number; height: number; rotation: number; flipX: boolean; flipY: boolean };
		faces: number[][];
		uniforms: number[];
	}[];
};

describe("face tracks through WASM", () => {
	it("gives the faces at a time like the reference", () => {
		for (const probe of cases.lookups) {
			const flat = Array.from(faceTrackFacesAt(cases.track, probe.time));
			const want = probe.faces.flat();
			expect(flat.length).toBe(want.length);
			flat.forEach((value, index) => expect(value).toBeCloseTo(want[index], 9));
		}
		expect(faceTrackMedia(cases.track)).toBe("face-media");
		for (const bad of cases.invalid) expect(faceTrackMedia(bad)).toBe("");
	});

	it("places the face ovals like the reference", () => {
		for (const c of cases.ovals) {
			const p = c.placement;
			const got = Array.from(retouchFaceUniforms(new Float64Array(c.faces.flat()), p.centerX, p.centerY, p.width, p.height, p.rotation, p.flipX, p.flipY));
			got.forEach((value, index) => expect(value).toBeCloseTo(c.uniforms[index], 9));
		}
	});

	it("fills faces-only passes and drops them with no faces", () => {
		const pass = { shader: "retouch", uniforms: { u_smooth: 0.6, u_face0_rx: 0 }, faces: true };
		const plain = { shader: "sharpen", uniforms: { u_amount: 1 } };
		const transform = { centerX: 960, centerY: 540, width: 1920, height: 1080, rotationDegrees: 0, flipX: false, flipY: false };
		const filled = withFaceOvals([[pass, plain]], [0.5, 0.4, 0.2, 0.3], transform);
		expect(filled[0][0].uniforms.u_face0_rx).toBeCloseTo(0.6 * 0.2 * 1920, 9);
		expect(filled[0][0]).not.toHaveProperty("faces");
		expect(filled[0][1]).toBe(plain);
		expect(withFaceOvals([[pass]], null, transform)).toEqual([]);
		expect(withFaceOvals([[pass, plain]], [], transform)).toEqual([[plain]]);
	});
});

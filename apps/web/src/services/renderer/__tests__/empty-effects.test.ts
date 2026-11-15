import { beforeAll, describe, expect, it } from "vitest";
import { registerDefaultEffects } from "@/effects";
import { resolveEffectPassGroups } from "@/services/renderer/resolve";
import type { Effect } from "@/effects/types";

const blur = (intensity: number): Effect => ({ id: `blur-${intensity}`, type: "blur", params: { intensity }, enabled: true }) as Effect;

describe("effects that do nothing", () => {
	beforeAll(() => registerDefaultEffects());

	// A blur animated down to 0 has no passes; the effects pipeline rejects an
	// empty pass list, which blacked out the whole frame (found by the studio).
	it("leaves out a blur at 0 and keeps a real one", () => {
		const groups = resolveEffectPassGroups({ effects: [blur(0), blur(15)], animations: undefined, localTime: 0, width: 640, height: 360 });
		expect(groups).toHaveLength(1);
		expect(groups[0].length).toBeGreaterThan(0);
		expect(resolveEffectPassGroups({ effects: [blur(0)], animations: undefined, localTime: 0, width: 640, height: 360 })).toEqual([]);
	});
});

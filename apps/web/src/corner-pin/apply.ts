import { hasKeyframesForPath, upsertPathKeyframe } from "@/animation";
import type { ElementAnimations } from "@/animation/types";
import { coerceParamValue, getParamChannelLayout, type ParamValues } from "@/params";
import { getElementParam } from "@/params/registry";
import type { TimelineElement } from "@/timeline";
import type { MediaTime } from "@/wasm";
import { CORNER_PIN_ENABLED, CORNER_PIN_KEYS, type CornerPin, cornerPinParams } from "./pin";

/** Video, picture and sticker clips take a corner pin. */
export function canCornerPin(element: TimelineElement): boolean {
	return element.type === "video" || element.type === "image" || element.type === "sticker";
}

/**
 * The element update that pins its corners and switches the pin on. A
 * corner axis that has keyframes gets a keyframe at `localTime` instead
 * of a new base value, as the property fields do.
 */
export function cornerPinUpdates({
	element,
	corners,
	localTime,
}: {
	element: TimelineElement;
	corners: CornerPin;
	localTime: MediaTime;
}): Partial<TimelineElement> {
	if (!("params" in element)) return {};
	const values = cornerPinParams(corners);
	const params: ParamValues = { ...element.params, [CORNER_PIN_ENABLED]: true };
	let animations = "animations" in element ? (element.animations as ElementAnimations | undefined) : undefined;
	let animated = false;
	for (const key of CORNER_PIN_KEYS) {
		const value = values[key];
		if (hasKeyframesForPath({ animations, propertyPath: key })) {
			const param = getElementParam({ element, key });
			if (!param) continue;
			animations = upsertPathKeyframe({
				animations,
				propertyPath: key,
				time: localTime,
				value,
				channelLayout: getParamChannelLayout({ param }),
				coerceValue: ({ value: next }) => coerceParamValue({ param, value: next }),
			});
			animated = true;
		} else {
			params[key] = value;
		}
	}
	return { params, ...(animated && { animations }) } as Partial<TimelineElement>;
}

/** The update that puts the corners back on the clip's own corners and drops their keyframes. */
export function resetCornerPinUpdates(element: TimelineElement): Partial<TimelineElement> {
	if (!("params" in element)) return {};
	const params: ParamValues = { ...element.params };
	for (const key of CORNER_PIN_KEYS) delete params[key];
	const current = "animations" in element ? (element.animations as ElementAnimations | undefined) : undefined;
	if (!current || !CORNER_PIN_KEYS.some((key) => key in current)) {
		return { params } as Partial<TimelineElement>;
	}
	const animations: ElementAnimations = { ...current };
	for (const key of CORNER_PIN_KEYS) delete animations[key];
	return { params, animations } as Partial<TimelineElement>;
}

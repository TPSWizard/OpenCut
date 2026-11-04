import type { TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import {
	getIncomingTransitionWindow,
	getOutgoingTransitionWindow,
	isVisualElement,
} from "@/transitions/window";

/**
 * The part of a transition that plays over this clip, drawn on the clip:
 * a translucent band at the end of a clip that has one and at the start of
 * the clip after it, so the cut reads as one shared span.
 */
export function TransitionMarkers({
	element,
	track,
	pixelsPerSecond,
}: {
	element: TimelineElement;
	track: TimelineTrack;
	pixelsPerSecond: number;
}) {
	if (!isVisualElement(element)) return null;
	const outgoing = getOutgoingTransitionWindow({ track, element });
	const incoming = getIncomingTransitionWindow({ track, element });
	const end = element.startTime + element.duration;
	const spans = [
		outgoing && { window: outgoing, outgoing: true },
		incoming && { window: incoming, outgoing: false },
	]
		.filter((span): span is { window: NonNullable<typeof outgoing>; outgoing: boolean } =>
			Boolean(span),
		)
		.map(({ window, outgoing }) => ({
			start: Math.max(window.start, element.startTime),
			end: Math.min(window.end, end),
			outgoing,
		}))
		.filter((span) => span.end > span.start);
	if (spans.length === 0) return null;
	const toPx = (ticks: number) => (ticks / TICKS_PER_SECOND) * pixelsPerSecond;
	return (
		<>
			{spans.map((span) => (
				<div
					key={span.outgoing ? "out" : "in"}
					className={
						span.outgoing
							? "pointer-events-none absolute inset-y-0 border-l border-white/70 bg-white/20"
							: "pointer-events-none absolute inset-y-0 border-r border-white/70 bg-white/20"
					}
					style={{
						left: `${toPx(span.start - element.startTime)}px`,
						width: `${Math.max(toPx(span.end - span.start), 2)}px`,
					}}
					title={
						span.outgoing
							? `Transition out: ${element.transition?.type ?? ""}`
							: "Transition in"
					}
				/>
			))}
		</>
	);
}

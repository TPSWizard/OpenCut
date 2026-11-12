/**
 * UI state for the timeline
 * For core logic, use EditorCore instead.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";

interface TimelineStore {
	snappingEnabled: boolean;
	toggleSnapping: () => void;
	rippleEditingEnabled: boolean;
	toggleRippleEditing: () => void;
	expandedElementIds: Set<string>;
	toggleElementExpanded: (elementId: string) => void;
	/** In and out marks in ticks; a range for ripple-deleting. Not saved. */
	markIn: number | null;
	markOut: number | null;
	setMarkIn: (time: number | null) => void;
	setMarkOut: (time: number | null) => void;
	clearMarks: () => void;
}

export const useTimelineStore = create<TimelineStore>()(
	persist(
		(set) => ({
			snappingEnabled: true,

			toggleSnapping: () => {
				set((state) => ({ snappingEnabled: !state.snappingEnabled }));
			},

			rippleEditingEnabled: false,

			toggleRippleEditing: () => {
				set((state) => ({
					rippleEditingEnabled: !state.rippleEditingEnabled,
				}));
			},

			markIn: null,
			markOut: null,
			setMarkIn: (time) =>
				set((state) => ({
					markIn: time,
					markOut: time !== null && state.markOut !== null && state.markOut <= time ? null : state.markOut,
				})),
			setMarkOut: (time) =>
				set((state) => ({
					markOut: time,
					markIn: time !== null && state.markIn !== null && state.markIn >= time ? null : state.markIn,
				})),
			clearMarks: () => set({ markIn: null, markOut: null }),
			expandedElementIds: new Set<string>(),

			toggleElementExpanded: (elementId) => {
				set((state) => {
					const next = new Set(state.expandedElementIds);
					if (next.has(elementId)) {
						next.delete(elementId);
					} else {
						next.add(elementId);
					}
					return { expandedElementIds: next };
				});
			},
		}),
		{
			name: "timeline-store",
			partialize: (state) => ({
				snappingEnabled: state.snappingEnabled,
				rippleEditingEnabled: state.rippleEditingEnabled,
			}),
		},
	),
);

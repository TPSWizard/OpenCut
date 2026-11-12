import { useCallback } from "react";
import { useEditor } from "@/editor/use-editor";
import type { ElementRef } from "@/timeline/types";
import { groupRefs } from "@/timeline/compound";

export function useElementSelection() {
	const editor = useEditor();
	const selectedElements = useEditor((e) => e.selection.getSelectedElements());

	const isElementSelected = useCallback(
		({ trackId, elementId }: ElementRef) =>
			selectedElements.some(
				(element) =>
					element.trackId === trackId && element.elementId === elementId,
			),
		[selectedElements],
	);

	/** The clips grouped with `ref` (itself included). */
	const groupOf = useCallback(
		(ref: ElementRef): ElementRef[] => {
			const tracks = editor.scenes.getActiveSceneOrNull()?.tracks;
			return tracks ? groupRefs({ tracks, ref }) : [ref];
		},
		[editor],
	);

	/** Selects the clip and the rest of its group. */
	const selectElement = useCallback(
		({ trackId, elementId }: ElementRef) => {
			editor.selection.setSelectedElements({
				elements: groupOf({ trackId, elementId }),
			});
		},
		[editor, groupOf],
	);

	/** Selects just this clip, even inside a group (Alt-click). */
	const selectOneElement = useCallback(
		({ trackId, elementId }: ElementRef) => {
			editor.selection.setSelectedElements({ elements: [{ trackId, elementId }] });
		},
		[editor],
	);

	const addElementToSelection = useCallback(
		({ trackId, elementId }: ElementRef) => {
			const alreadySelected = selectedElements.some(
				(element) =>
					element.trackId === trackId && element.elementId === elementId,
			);
			if (alreadySelected) return;

			editor.selection.setSelectedElements({
				elements: [...selectedElements, { trackId, elementId }],
			});
		},
		[selectedElements, editor],
	);

	const removeElementFromSelection = useCallback(
		({ trackId, elementId }: ElementRef) => {
			editor.selection.setSelectedElements({
				elements: selectedElements.filter(
					(element) =>
						!(element.trackId === trackId && element.elementId === elementId),
				),
			});
		},
		[selectedElements, editor],
	);

	const toggleElementSelection = useCallback(
		({ trackId, elementId }: ElementRef) => {
			// A grouped clip toggles its whole group.
			const members = groupOf({ trackId, elementId });
			const isIn = (ref: ElementRef) =>
				selectedElements.some((element) => element.trackId === ref.trackId && element.elementId === ref.elementId);
			if (members.every(isIn)) {
				editor.selection.setSelectedElements({
					elements: selectedElements.filter(
						(element) => !members.some((member) => member.trackId === element.trackId && member.elementId === element.elementId),
					),
				});
			} else {
				editor.selection.setSelectedElements({
					elements: [...selectedElements, ...members.filter((member) => !isIn(member))],
				});
			}
		},
		[selectedElements, editor, groupOf],
	);

	const clearElementSelection = useCallback(() => {
		editor.selection.clearSelection();
	}, [editor]);

	const setElementSelection = useCallback(
		({ elements }: { elements: ElementRef[] }) => {
			editor.selection.setSelectedElements({ elements });
		},
		[editor],
	);


	/**
	 * Merges elements into the current selection, deduplicating by identity.
	 * Used for additive box-select where the pre-drag selection is preserved.
	 */
	const mergeElementsIntoSelection = useCallback(
		({ elements }: { elements: ElementRef[] }) => {
			const merged = [
				...selectedElements.filter(
					(selectedElement) =>
						!elements.some(
							(element) =>
								element.trackId === selectedElement.trackId &&
								element.elementId === selectedElement.elementId,
						),
				),
				...elements,
			];
			editor.selection.setSelectedElements({ elements: merged });
		},
		[selectedElements, editor],
	);


	/**
	 * Handles click interaction on an element.
	 * - Regular click: select only this element
	 * - Multi-key click (Ctrl/Cmd): toggle this element in selection
	 */
	const handleElementClick = useCallback(
		({
			trackId,
			elementId,
			isMultiKey,
		}: ElementRef & { isMultiKey: boolean }) => {
			if (isMultiKey) {
				toggleElementSelection({ trackId, elementId });
			} else {
				selectElement({ trackId, elementId });
			}
		},
		[toggleElementSelection, selectElement],
	);

	return {
		groupOf,
		selectOneElement,
		selectedElements,
		isElementSelected,
		selectElement,
		setElementSelection,
		mergeElementsIntoSelection,
		addElementToSelection,
		removeElementFromSelection,
		toggleElementSelection,
		clearElementSelection,
		handleElementClick,
	};
}

"use client";

import { useEffect } from "react";

/**
 * Installs the Editor API bridge (`window.opencut`) once for the whole app so
 * it is available on the projects page too, not only inside an open project.
 * The editor core is loaded lazily so the landing page does not pay for it.
 */
export function EditorApiRoot() {
	useEffect(() => {
		let cancelled = false;
		void Promise.all([import("@/core"), import("./bridge")]).then(
			([{ EditorCore }, { installEditorApiBridge }]) => {
				if (cancelled || window.opencut) return;
				installEditorApiBridge({ editor: EditorCore.getInstance() });
			},
		);
		return () => {
			cancelled = true;
		};
	}, []);

	return null;
}

"use client";

import { useEffect } from "react";
import { registerServiceWorker, watchInstallPrompt } from "../register";

/** Mounts once in the root layout: registers the worker and watches the install prompt. */
export function PwaRoot() {
	useEffect(() => {
		void registerServiceWorker();
		return watchInstallPrompt();
	}, []);
	return null;
}

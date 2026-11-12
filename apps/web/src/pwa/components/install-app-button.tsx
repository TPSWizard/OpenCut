"use client";

import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { promptInstall, usePwaStore } from "../register";

/** Shown only while the browser offers to install the app. */
export function InstallAppButton() {
	const installPrompt = usePwaStore((state) => state.installPrompt);
	const installed = usePwaStore((state) => state.installed);
	const [busy, setBusy] = useState(false);
	if (installed || !installPrompt) return null;
	return (
		<Button
			variant="outline"
			size="lg"
			className="flex px-5 md:px-6"
			disabled={busy}
			data-testid="install-app"
			title="Install ConzaCut as an app; it then opens .opencut.zip bundles and works offline"
			onClick={async () => {
				setBusy(true);
				try {
					if (await promptInstall()) toast.success("ConzaCut installed");
				} finally {
					setBusy(false);
				}
			}}
		>
			<span className="text-sm font-medium">Install app</span>
		</Button>
	);
}

"use client";

import { useEffect, useRef } from "react";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { useEditorApiBridge } from "@/api/bridge-store";
import type { InstalledPlugin } from "../types";

/**
 * Hosts a plugin's panel page in a sandboxed iframe. The frame has an opaque
 * origin (no `allow-same-origin`), so it cannot touch editor storage; it
 * only reaches the editor through the Editor API bridge.
 */
export function PluginPanelView({ plugin }: { plugin: InstalledPlugin }) {
	const bridge = useEditorApiBridge();
	const frameRef = useRef<HTMLIFrameElement>(null);

	useEffect(() => {
		const frame = frameRef.current;
		if (!bridge || !frame) return;
		let release: (() => void) | null = null;
		const allow = () => {
			release?.();
			release = frame.contentWindow ? bridge.allowSource(frame.contentWindow) : null;
		};
		frame.addEventListener("load", allow);
		if (frame.contentWindow) allow();
		return () => {
			frame.removeEventListener("load", allow);
			release?.();
		};
	}, [bridge, plugin.panelUrl]);

	if (!plugin.panelUrl) {
		return (
			<PanelView title={plugin.manifest.name}>
				<p className="text-muted-foreground p-2 text-sm">
					This plugin has no panel.
				</p>
			</PanelView>
		);
	}

	return (
		<PanelView
			title={plugin.manifest.panel?.title ?? plugin.manifest.name}
			contentClassName="h-full p-0"
			scrollClassName="overflow-hidden pt-0"
		>
			<iframe
				ref={frameRef}
				title={`${plugin.manifest.name} plugin`}
				src={plugin.panelUrl}
				sandbox="allow-scripts allow-forms allow-downloads"
				className="bg-background block h-full w-full border-0"
			/>
		</PanelView>
	);
}

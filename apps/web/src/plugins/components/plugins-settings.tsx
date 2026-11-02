"use client";

import { useState } from "react";
import { toast } from "sonner";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete02Icon, PlusSignIcon } from "@hugeicons/core-free-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
	Section,
	SectionContent,
	SectionHeader,
	SectionTitle,
} from "@/components/section";
import { useAssetsPanelStore } from "@/components/editor/panels/assets/assets-panel-store";
import { usePluginsStore } from "../plugins-store";
import { pluginTabKey } from "../types";

export const SAMPLE_PLUGIN_MANIFEST_URL = "/plugins/timeline-stats/manifest.json";

export function PluginsSettings() {
	const { plugins, status, install, remove, setEnabled } = usePluginsStore();
	const setActiveTab = useAssetsPanelStore((state) => state.setActiveTab);
	const [manifestUrl, setManifestUrl] = useState("");
	const [isInstalling, setIsInstalling] = useState(false);

	const handleInstall = async (url: string) => {
		const trimmed = url.trim();
		if (!trimmed || isInstalling) return;
		setIsInstalling(true);
		try {
			const installed = await install({ manifestUrl: trimmed });
			toast.success(`Installed ${installed.manifest.name}`);
			setManifestUrl("");
			if (installed.panelUrl) {
				setActiveTab(pluginTabKey(installed.manifest.id));
			}
		} catch (error) {
			toast.error("Could not install plugin", {
				description: error instanceof Error ? error.message : "Unknown error",
			});
		} finally {
			setIsInstalling(false);
		}
	};

	return (
		<div className="flex flex-col gap-4">
			<Section sectionKey="plugins-install" defaultOpen>
				<SectionHeader>
					<SectionTitle>Install a plugin</SectionTitle>
				</SectionHeader>
				<SectionContent className="flex flex-col gap-2">
					<p className="text-muted-foreground text-xs">
						Paste the URL of a plugin manifest. Plugins run sandboxed and only
						reach the editor through the Editor API.
					</p>
					<form
						className="flex gap-2"
						onSubmit={(event) => {
							event.preventDefault();
							void handleInstall(manifestUrl);
						}}
					>
						<Input
							value={manifestUrl}
							onChange={(event) => setManifestUrl(event.target.value)}
							placeholder="https://example.com/plugin/manifest.json"
							className="h-8 text-xs"
						/>
						<Button
							type="submit"
							size="sm"
							className="h-8 gap-1"
							disabled={isInstalling || manifestUrl.trim().length === 0}
						>
							<HugeiconsIcon icon={PlusSignIcon} className="size-3.5" />
							Install
						</Button>
					</form>
					{!plugins.some((plugin) => plugin.manifestUrl.endsWith(SAMPLE_PLUGIN_MANIFEST_URL)) && (
						<Button
							variant="outline"
							size="sm"
							className="h-8 w-fit text-xs"
							disabled={isInstalling}
							onClick={() => void handleInstall(SAMPLE_PLUGIN_MANIFEST_URL)}
						>
							Install the sample “Timeline stats” plugin
						</Button>
					)}
				</SectionContent>
			</Section>

			<Section sectionKey="plugins-installed" defaultOpen>
				<SectionHeader>
					<SectionTitle>Installed</SectionTitle>
				</SectionHeader>
				<SectionContent className="flex flex-col gap-2">
					{plugins.length === 0 && (
						<p className="text-muted-foreground text-xs">No plugins installed.</p>
					)}
					{plugins.map((plugin) => {
						const pluginStatus = status[plugin.manifest.id] ?? { state: "idle" };
						return (
							<div
								key={plugin.manifest.id}
								className="bg-accent/40 flex flex-col gap-1 rounded-md border p-2"
							>
								<div className="flex items-center justify-between gap-2">
									<div className="min-w-0">
										<div className="truncate text-sm font-medium">
											{plugin.manifest.name}{" "}
											<span className="text-muted-foreground text-xs font-normal">
												v{plugin.manifest.version}
											</span>
										</div>
										{plugin.manifest.description && (
											<p className="text-muted-foreground line-clamp-2 text-xs">
												{plugin.manifest.description}
											</p>
										)}
									</div>
									<div className="flex shrink-0 items-center gap-1">
										<Switch
											checked={plugin.enabled}
											onCheckedChange={(enabled) =>
												setEnabled({ pluginId: plugin.manifest.id, enabled })
											}
											aria-label={`Enable ${plugin.manifest.name}`}
										/>
										<Button
											variant="ghost"
											size="icon"
											className="size-7"
											aria-label={`Remove ${plugin.manifest.name}`}
											onClick={() => remove({ pluginId: plugin.manifest.id })}
										>
											<HugeiconsIcon icon={Delete02Icon} className="size-3.5" />
										</Button>
									</div>
								</div>
								<div className="text-muted-foreground flex flex-wrap items-center gap-x-3 text-[0.7rem]">
									{plugin.panelUrl && plugin.enabled && (
										<button
											type="button"
											className="hover:text-foreground underline underline-offset-2"
											onClick={() => setActiveTab(pluginTabKey(plugin.manifest.id))}
										>
											Open panel
										</button>
									)}
									{plugin.entryUrl && (
										<span>
											Background:{" "}
											{pluginStatus.state === "running"
												? "running"
												: pluginStatus.state === "error"
													? `error — ${pluginStatus.message}`
													: plugin.enabled
														? "starting"
														: "off"}
										</span>
									)}
									<span className="truncate">{plugin.manifestUrl}</span>
								</div>
							</div>
						);
					})}
				</SectionContent>
			</Section>
		</div>
	);
}

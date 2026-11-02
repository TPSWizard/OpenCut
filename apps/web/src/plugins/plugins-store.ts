import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
	type InstalledPlugin,
	parsePluginManifest,
	PluginManifestError,
	resolvePluginUrl,
	type PluginStatus,
} from "./types";

interface PluginsState {
	plugins: InstalledPlugin[];
	/** Runtime status per plugin id (not persisted). */
	status: Record<string, PluginStatus>;
	isHydrated: boolean;
	install: ({ manifestUrl }: { manifestUrl: string }) => Promise<InstalledPlugin>;
	remove: ({ pluginId }: { pluginId: string }) => void;
	setEnabled: ({ pluginId, enabled }: { pluginId: string; enabled: boolean }) => void;
	setStatus: ({ pluginId, status }: { pluginId: string; status: PluginStatus }) => void;
	setHydrated: () => void;
}

export async function fetchPluginManifest({
	manifestUrl,
}: {
	manifestUrl: string;
}): Promise<Omit<InstalledPlugin, "enabled" | "installedAt">> {
	let absolute: string;
	try {
		absolute = new URL(manifestUrl, window.location.href).toString();
	} catch {
		throw new PluginManifestError("Enter a valid manifest URL");
	}
	const response = await fetch(absolute, { cache: "no-store" });
	if (!response.ok) {
		throw new PluginManifestError(
			`Could not fetch the manifest (${response.status} ${response.statusText})`,
		);
	}
	let json: unknown;
	try {
		json = await response.json();
	} catch {
		throw new PluginManifestError("The manifest is not valid JSON");
	}
	const manifest = parsePluginManifest(json);
	return {
		manifestUrl: absolute,
		manifest,
		panelUrl: manifest.panel
			? resolvePluginUrl({ base: absolute, url: manifest.panel.url })
			: undefined,
		entryUrl: manifest.entry
			? resolvePluginUrl({ base: absolute, url: manifest.entry })
			: undefined,
	};
}

export const usePluginsStore = create<PluginsState>()(
	persist(
		(set, get) => ({
			plugins: [],
			status: {},
			isHydrated: false,
			install: async ({ manifestUrl }) => {
				const fetched = await fetchPluginManifest({ manifestUrl });
				const existing = get().plugins.find(
					(plugin) => plugin.manifest.id === fetched.manifest.id,
				);
				const installed: InstalledPlugin = {
					...fetched,
					enabled: existing?.enabled ?? true,
					installedAt: existing?.installedAt ?? new Date().toISOString(),
				};
				set((state) => ({
					plugins: [
						...state.plugins.filter(
							(plugin) => plugin.manifest.id !== installed.manifest.id,
						),
						installed,
					],
				}));
				return installed;
			},
			remove: ({ pluginId }) =>
				set((state) => {
					const status = { ...state.status };
					delete status[pluginId];
					return {
						plugins: state.plugins.filter(
							(plugin) => plugin.manifest.id !== pluginId,
						),
						status,
					};
				}),
			setEnabled: ({ pluginId, enabled }) =>
				set((state) => ({
					plugins: state.plugins.map((plugin) =>
						plugin.manifest.id === pluginId ? { ...plugin, enabled } : plugin,
					),
				})),
			setStatus: ({ pluginId, status }) =>
				set((state) => ({ status: { ...state.status, [pluginId]: status } })),
			setHydrated: () => set({ isHydrated: true }),
		}),
		{
			name: "opencut-plugins",
			partialize: (state) => ({ plugins: state.plugins }),
			onRehydrateStorage: () => (state) => {
				state?.setHydrated();
			},
		},
	),
);

export function useEnabledPlugins(): InstalledPlugin[] {
	return usePluginsStore((state) => state.plugins).filter((plugin) => plugin.enabled);
}

"use client";

import { Separator } from "@/components/ui/separator";
import {
	type BuiltinTab,
	useAssetsPanelStore,
} from "@/components/editor/panels/assets/assets-panel-store";
import { ScriptingView } from "@/scripting/components/scripting-view";
import { PluginPanelView } from "@/plugins/components/plugin-panel-view";
import { useEnabledPlugins } from "@/plugins/plugins-store";
import { isPluginTabKey, pluginIdFromTabKey } from "@/plugins/types";
import { TabBar } from "./tabbar";
import { Captions } from "@/subtitles/components/assets-view";
import { VoiceoverView } from "@/voiceover/components/voiceover-view";
import { MediaView } from "./views/assets";
import { SettingsView } from "./views/settings";
import { SoundsView } from "@/sounds/components/assets-view";
import { StickersView } from "@/stickers/components/assets-view";
import { StockView } from "@/stock/components/stock-view";
import { TextView } from "@/text/components/assets-view";
import { EffectsView } from "@/effects/components/assets-view";
import { TransitionsView } from "@/transitions/components/assets-view";
import { MarkersView } from "@/timeline/bookmarks/components/markers-view";

export function AssetsPanel() {
	const { activeTab } = useAssetsPanelStore();
	const plugins = useEnabledPlugins();

	const viewMap: Record<BuiltinTab, React.ReactNode> = {
		media: <MediaView />,
		sounds: <SoundsView />,
		text: <TextView />,
		stickers: <StickersView />,
		stock: <StockView />,
		effects: <EffectsView />,
		transitions: <TransitionsView />,
		captions: <Captions />,
		voiceover: <VoiceoverView />,
		markers: <MarkersView />,
		adjustment: (
			<div className="text-muted-foreground p-4">
				Adjustment view coming soon...
			</div>
		),
		scripting: <ScriptingView />,
		settings: <SettingsView />,
	};

	let view: React.ReactNode;
	if (isPluginTabKey(activeTab)) {
		const pluginId = pluginIdFromTabKey(activeTab);
		const plugin = plugins.find((item) => item.manifest.id === pluginId);
		view = plugin ? <PluginPanelView plugin={plugin} /> : viewMap.media;
	} else {
		view = viewMap[activeTab];
	}

	return (
		<div className="panel bg-background flex h-full rounded-sm border overflow-hidden">
			<TabBar />
			<Separator orientation="vertical" />
			<div className="flex-1 overflow-hidden">{view}</div>
		</div>
	);
}

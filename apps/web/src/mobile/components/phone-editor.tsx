"use client";

import { useEffect, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
	ClosedCaptionIcon,
	Copy01Icon,
	Delete02Icon,
	Folder03Icon,
	Happy01Icon,
	HeadphonesIcon,
	Image02Icon,
	MagicWand05Icon,
	Mic01Icon,
	PencilEdit02Icon,
	Redo02Icon,
	ScissorIcon,
	Settings01Icon,
	TextIcon,
	Tick02Icon,
	Undo02Icon,
} from "@hugeicons/core-free-icons";
import type { IconSvgElement } from "@hugeicons/react";
import { invokeAction } from "@/actions";
import { Button } from "@/components/ui/button";
import {
	Drawer,
	DrawerContent,
	DrawerTitle,
} from "@/components/ui/drawer";
import { ProjectDropdown } from "@/components/editor/editor-header";
import { ExportButton } from "@/components/editor/export-button";
import { AssetsTabContent } from "@/components/editor/panels/assets";
import {
	TAB_KEYS,
	tabs,
	useAssetsPanelStore,
	type BuiltinTab,
} from "@/components/editor/panels/assets/assets-panel-store";
import { TAB_LABELS } from "@/components/editor/panels/assets/tabbar";
import { PropertiesPanel } from "@/components/editor/panels/properties";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { cn } from "@/utils/ui";
import { PhoneTimeline } from "./phone-timeline";

/** Tabs the phone's assets sheet offers (the desktop's, less the unfinished one). */
const SHEET_TABS = TAB_KEYS.filter((key) => key !== "adjustment");

type Sheet = "assets" | "properties" | null;

/**
 * The editor on a phone: the preview on top, the touch timeline under it,
 * and a toolbar along the bottom. The desktop's assets and properties panels
 * open in bottom sheets, so every tool is still there; the project menu
 * switches to the desktop layout.
 */
export function PhoneEditor({ preview }: { preview: React.ReactNode }) {
	const t = useT();
	const projectName = useEditor((e) => e.project.getActiveOrNull()?.metadata.name ?? "");
	const canUndo = useEditor((e) => e.command.canUndo());
	const canRedo = useEditor((e) => e.command.canRedo());
	const selectedCount = useEditor((e) => e.selection.getSelectedElements().length);
	const { activeTab, setActiveTab } = useAssetsPanelStore();
	const [sheet, setSheet] = useState<Sheet>(null);

	// Nothing selected: the properties sheet would be empty.
	useEffect(() => {
		if (selectedCount === 0 && sheet === "properties") setSheet(null);
	}, [selectedCount, sheet]);

	const openTab = (tab: BuiltinTab) => {
		setActiveTab(tab);
		setSheet("assets");
	};

	const addTools: { key: BuiltinTab; icon: IconSvgElement }[] = [
		{ key: "media", icon: Folder03Icon },
		{ key: "text", icon: TextIcon },
		{ key: "sounds", icon: HeadphonesIcon },
		{ key: "stock", icon: Image02Icon },
		{ key: "stickers", icon: Happy01Icon },
		{ key: "captions", icon: ClosedCaptionIcon },
		{ key: "effects", icon: MagicWand05Icon },
		{ key: "voiceover", icon: Mic01Icon },
		{ key: "settings", icon: Settings01Icon },
	];

	return (
		<div
			className="bg-background flex h-dvh w-screen flex-col overflow-hidden"
			data-testid="phone-editor"
		>
			<header className="flex h-12 shrink-0 items-center gap-1 border-b px-1.5">
				<ProjectDropdown />
				<span className="min-w-0 flex-1 truncate text-sm font-medium">
					{projectName}
				</span>
				<Button
					variant="ghost"
					size="icon"
					aria-label={t("Undo")}
					disabled={!canUndo}
					onClick={() => invokeAction("undo")}
				>
					<HugeiconsIcon icon={Undo02Icon} />
				</Button>
				<Button
					variant="ghost"
					size="icon"
					aria-label={t("Redo")}
					disabled={!canRedo}
					onClick={() => invokeAction("redo")}
				>
					<HugeiconsIcon icon={Redo02Icon} />
				</Button>
				<ExportButton />
			</header>

			<div className="h-[36dvh] min-h-44 shrink-0 p-1.5">{preview}</div>

			<PhoneTimeline />

			<nav
				className="bg-background flex shrink-0 gap-1 overflow-x-auto border-t px-1.5 pt-1.5 pb-[max(0.375rem,env(safe-area-inset-bottom))] [scrollbar-width:none]"
				aria-label={t("Tools")}
				data-testid="phone-toolbar"
			>
				{selectedCount > 0 ? (
					<>
						<ToolButton
							icon={ScissorIcon}
							label={t("Split")}
							onClick={() => invokeAction("split")}
						/>
						<ToolButton
							icon={PencilEdit02Icon}
							label={t("Edit")}
							onClick={() => setSheet("properties")}
						/>
						<ToolButton
							icon={Copy01Icon}
							label={t("Duplicate")}
							onClick={() => invokeAction("duplicate-selected")}
						/>
						<ToolButton
							icon={Delete02Icon}
							label={t("Delete")}
							onClick={() => invokeAction("delete-selected")}
						/>
						<ToolButton
							icon={Tick02Icon}
							label={t("Done")}
							onClick={() => invokeAction("deselect-all")}
						/>
					</>
				) : (
					<>
						{addTools.map((tool) => (
							<ToolButton
								key={tool.key}
								icon={tool.icon}
								label={TAB_LABELS[tool.key]()}
								onClick={() => openTab(tool.key)}
							/>
						))}
					</>
				)}
			</nav>

			<Drawer
				open={sheet === "assets"}
				onOpenChange={(open) => setSheet(open ? "assets" : null)}
				handleOnly
			>
				<DrawerContent className="h-[70dvh]" data-testid="phone-assets-sheet">
					<DrawerTitle className="sr-only">{t("Add")}</DrawerTitle>
					<div className="flex shrink-0 gap-1 overflow-x-auto px-3 pt-3 pb-2 [scrollbar-width:none]">
						{SHEET_TABS.map((key) => {
							const Icon = tabs[key].icon;
							return (
								<button
									key={key}
									type="button"
									className={cn(
										"flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs",
										activeTab === key
											? "bg-primary text-primary-foreground border-primary"
											: "text-muted-foreground",
									)}
									onClick={() => setActiveTab(key)}
								>
									<Icon className="size-3.5" />
									{TAB_LABELS[key]()}
								</button>
							);
						})}
					</div>
					<div className="min-h-0 flex-1 overflow-hidden px-1 pb-2">
						<AssetsTabContent />
					</div>
				</DrawerContent>
			</Drawer>

			<Drawer
				open={sheet === "properties"}
				onOpenChange={(open) => setSheet(open ? "properties" : null)}
				handleOnly
			>
				<DrawerContent className="h-[70dvh]" data-testid="phone-properties-sheet">
					<DrawerTitle className="sr-only">{t("Edit")}</DrawerTitle>
					<div className="min-h-0 flex-1 overflow-hidden px-1 pt-2 pb-2">
						<PropertiesPanel />
					</div>
				</DrawerContent>
			</Drawer>
		</div>
	);
}

function ToolButton({
	icon,
	label,
	onClick,
}: {
	icon: IconSvgElement;
	label: string;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			className="text-foreground active:bg-accent flex min-w-16 shrink-0 flex-col items-center gap-1 rounded-md px-2 py-1.5"
			onClick={onClick}
		>
			<HugeiconsIcon icon={icon} className="size-5" />
			<span className="text-[11px] leading-none whitespace-nowrap">{label}</span>
		</button>
	);
}

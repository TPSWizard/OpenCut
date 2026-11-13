"use client";

import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { downloadBlob } from "@/utils/browser";
import type { AssetEntry, AssetManifest } from "../assets";
import { projectAssetManifest } from "../project-assets";
import { copyProjectCredits } from "./copy-credits";

const KIND_ORDER: AssetEntry["kind"][] = ["media", "sound", "sticker", "font"];

/** What the project uses, where each thing came from, its licence and the credits to give. */
export function AssetLicencesDialog({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const [manifest, setManifest] = useState<AssetManifest | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		if (!isOpen) return;
		let cancelled = false;
		setManifest(null);
		setError(null);
		projectAssetManifest({ editor }).then(
			(result) => !cancelled && setManifest(result),
			(failure) => !cancelled && setError(failure instanceof Error ? failure.message : String(failure)),
		);
		return () => {
			cancelled = true;
		};
	}, [isOpen, editor]);

	const kindLabel: Record<AssetEntry["kind"], string> = {
		media: t("Media"),
		sound: t("Library sounds"),
		sticker: t("Stickers"),
		font: t("Fonts"),
	};

	return (
		<Dialog open={isOpen} onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[80vh] max-w-2xl flex-col p-0" data-testid="asset-licences">
				<DialogHeader>
					<DialogTitle>{t("Credits and licences")}</DialogTitle>
					<DialogDescription>
						{t("Everything this project uses, where it came from and its licence. Exported projects carry this list as assets.json.")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="scrollbar-thin grow overflow-y-auto">
					{!manifest && !error && (
						<div className="flex justify-center py-6">
							<Spinner />
						</div>
					)}
					{error && <p className="text-destructive text-sm">{error}</p>}
					{manifest && (
						<div className="flex flex-col gap-4 text-sm">
							<p className="text-muted-foreground text-xs" data-testid="asset-summary">
								{t("{count} items · {credit} need a credit · {unknown} with no recorded licence", {
									count: manifest.summary.assets,
									credit: manifest.summary.creditRequired,
									unknown: manifest.summary.licenseUnknown,
								})}
							</p>
							{KIND_ORDER.map((kind) => {
								const entries = manifest.assets.filter((entry) => entry.kind === kind);
								if (entries.length === 0) return null;
								return (
									<section key={kind} className="flex flex-col gap-1">
										<h3 className="text-xs font-medium">{kindLabel[kind]}</h3>
										<ul className="divide-y rounded-md border">
											{entries.map((entry) => (
												<li key={`${entry.kind}:${entry.id}`} className="flex flex-col gap-0.5 px-2 py-1.5">
													<div className="flex items-center gap-2">
														<span className="truncate font-medium">{entry.name}</span>
														<span className="text-muted-foreground ml-auto shrink-0 text-xs">
															{entry.license ?? (entry.origin === "system" ? t("System font") : t("Not recorded"))}
														</span>
														{entry.creditRequired && (
															<span className="bg-accent shrink-0 rounded px-1.5 text-[10px]">{t("Credit")}</span>
														)}
													</div>
													{(entry.attribution || entry.note) && (
														<span className="text-muted-foreground text-xs">{entry.attribution ?? entry.note}</span>
													)}
													{entry.sourceUrl?.startsWith("http") && (
														<a
															href={entry.sourceUrl}
															target="_blank"
															rel="noopener noreferrer"
															className="text-muted-foreground truncate text-[10px] underline"
														>
															{entry.sourceUrl}
														</a>
													)}
												</li>
											))}
										</ul>
									</section>
								);
							})}
						</div>
					)}
				</DialogBody>
				<DialogFooter className="flex items-center gap-2">
					<Button
						variant="outline"
						disabled={!manifest}
						onClick={() =>
							manifest &&
							downloadBlob({
								blob: new Blob([JSON.stringify(manifest, null, 2)], { type: "application/json" }),
								filename: "assets.json",
							})
						}
					>
						{t("Download assets.json")}
					</Button>
					<Button disabled={!manifest} onClick={() => copyProjectCredits({ editor, t })} data-testid="asset-copy-credits">
						{t("Copy credits")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

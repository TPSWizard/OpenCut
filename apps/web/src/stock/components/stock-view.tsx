"use client";

import { useState } from "react";
import { toast } from "sonner";
import { PanelView } from "@/components/editor/panels/assets/views/base-panel";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { cn } from "@/utils/ui";
import {
	StockUnavailableError,
	importStockItem,
	projectCredits,
	searchStock,
	type StockItem,
	type StockKind,
	type StockProvider,
} from "../stock";

/** Stock pictures and videos: search, see the licence and creator, add at the playhead. */
export function StockView() {
	const t = useT();
	const editor = useEditor();
	const [query, setQuery] = useState("");
	const [provider, setProvider] = useState<StockProvider>("openverse");
	const [kind, setKind] = useState<StockKind>("image");
	const [items, setItems] = useState<StockItem[]>([]);
	const [page, setPage] = useState(1);
	const [total, setTotal] = useState(0);
	const [busy, setBusy] = useState(false);
	const [adding, setAdding] = useState<string | null>(null);
	const [note, setNote] = useState<string | null>(null);

	const run = async ({ nextPage = 1, nextProvider = provider, nextKind = kind }: { nextPage?: number; nextProvider?: StockProvider; nextKind?: StockKind } = {}) => {
		if (!query.trim()) return;
		setBusy(true);
		setNote(null);
		try {
			const result = await searchStock({ provider: nextProvider, kind: nextKind, query, page: nextPage });
			setItems((current) => (nextPage === 1 ? result.items : [...current, ...result.items]));
			setTotal(result.total);
			setPage(nextPage);
			if (result.items.length === 0 && nextPage === 1) setNote(t("Nothing found."));
		} catch (error) {
			if (nextPage === 1) setItems([]);
			setNote(
				error instanceof StockUnavailableError
					? nextProvider === "pixabay"
						? t("Pixabay is not set up on this server yet (it needs an API key).")
						: t("Openverse has pictures only; videos come from Pixabay.")
					: error instanceof Error
						? error.message
						: t("Search failed"),
			);
		} finally {
			setBusy(false);
		}
	};

	const add = async (item: StockItem) => {
		setAdding(`${item.provider}:${item.id}`);
		try {
			const result = await importStockItem({ editor, item });
			toast.success(t("Added {name}", { name: result.name }), { description: item.attribution });
		} catch (error) {
			toast.error(t("Could not add it"), { description: error instanceof Error ? error.message : String(error) });
		} finally {
			setAdding(null);
		}
	};

	const copyCredits = () => {
		const credits = projectCredits({ editor });
		if (!credits) {
			toast.info(t("No stock media in this project yet."));
			return;
		}
		void navigator.clipboard.writeText(credits).then(
			() => toast.success(t("Credits copied"), { description: t("Paste them into your video's description.") }),
			() => toast.error(t("The clipboard is not available here")),
		);
	};

	const choice = <T extends string>(value: T, current: T, label: string, onPick: (value: T) => void, testId: string) => (
		<button
			type="button"
			data-testid={testId}
			className={cn("rounded-md border px-2 py-0.5 text-xs", value === current ? "bg-accent border-foreground/30" : "text-muted-foreground")}
			onClick={() => onPick(value)}
		>
			{label}
		</button>
	);

	return (
		<PanelView
			title={t("Stock")}
			actions={
				<Button size="sm" variant="outline" onClick={copyCredits} data-testid="stock-credits">
					{t("Copy credits")}
				</Button>
			}
		>
			<div className="flex flex-col gap-3" data-testid="stock-view">
				<form
					className="flex gap-2"
					onSubmit={(event) => {
						event.preventDefault();
						void run();
					}}
				>
					<Input
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("Search pictures and videos")}
						className="h-8 text-xs"
						data-testid="stock-query"
					/>
					<Button size="sm" type="submit" disabled={busy || !query.trim()}>
						{busy ? <Spinner /> : t("Search")}
					</Button>
				</form>
				<div className="flex flex-wrap items-center gap-1">
					{choice<StockProvider>("openverse", provider, "Openverse", (value) => {
						setProvider(value);
						setKind("image");
						void run({ nextProvider: value, nextKind: "image" });
					}, "stock-openverse")}
					{choice<StockProvider>("pixabay", provider, "Pixabay", (value) => {
						setProvider(value);
						void run({ nextProvider: value });
					}, "stock-pixabay")}
					<span className="mx-1" />
					{choice<StockKind>("image", kind, t("Pictures"), (value) => {
						setKind(value);
						void run({ nextKind: value });
					}, "stock-images")}
					{choice<StockKind>("video", kind, t("Videos"), (value) => {
						setKind(value);
						void run({ nextKind: value });
					}, "stock-videos")}
				</div>
				<p className="text-muted-foreground text-xs">
					{provider === "openverse"
						? t("Pictures from Openverse that are free to use and change, even commercially (CC0, public domain, CC BY). Most need a credit: it is kept with each one.")
						: t("Pictures and videos from Pixabay, under the Pixabay Content License. The credit is kept with each one.")}
				</p>
				{note && (
					<p className="text-muted-foreground text-xs" role="status" data-testid="stock-note">
						{note}
					</p>
				)}
				<div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))" }} data-testid="stock-results">
					{items.map((item) => {
						const key = `${item.provider}:${item.id}`;
						return (
							<button
								key={key}
								type="button"
								onClick={() => void add(item)}
								disabled={adding !== null}
								title={item.attribution}
								className="bg-muted/40 hover:bg-muted relative flex flex-col overflow-hidden rounded-md border text-left"
								data-testid="stock-item"
							>
								{item.thumbnail ? (
									<img src={item.thumbnail} alt={item.title} loading="lazy" className="aspect-video w-full object-cover" />
								) : (
									<div className="bg-muted aspect-video w-full" />
								)}
								<span className="truncate px-1.5 pt-1 text-xs font-medium">{item.title}</span>
								<span className="text-muted-foreground truncate px-1.5 pb-1 text-[10px]">
									{item.creator} · {item.license}
									{item.duration ? ` · ${Math.round(item.duration)} s` : ""}
								</span>
								{adding === key && (
									<span className="bg-background/70 absolute inset-0 flex items-center justify-center">
										<Spinner />
									</span>
								)}
							</button>
						);
					})}
				</div>
				{items.length > 0 && items.length < total && (
					<Button size="sm" variant="outline" disabled={busy} onClick={() => void run({ nextPage: page + 1 })}>
						{t("More")}
					</Button>
				)}
				{items.length > 0 && (
					<p className="text-muted-foreground text-[10px]">
						{provider === "openverse" ? t("Results from Openverse (openverse.org).") : t("Results from Pixabay (pixabay.com).")}
					</p>
				)}
			</div>
		</PanelView>
	);
}

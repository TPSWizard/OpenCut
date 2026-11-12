"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
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
import { Progress } from "@/components/ui/progress";
import { useEditor } from "@/editor/use-editor";
import { downloadBlob } from "@/utils/browser";
import { cn } from "@/utils/ui";
import {
	type DurabilityAssessment,
	describeBackupAge,
	needsBackup,
	readBackups,
	readDurability,
	requestPersistence,
} from "@/services/storage/durability";
import { formatStorageBytes, readStorageQuotaStatus } from "@/services/storage/quota";
import { storageService } from "@/services/storage/service";
import { useT } from "@/i18n";

interface ProjectStorageRow {
	id: string;
	name: string;
	updatedAt: number;
	mediaBytes: number;
	proxyBytes: number;
	missing: number;
	lastBackupAt: number | null;
}

type Durability = Awaited<ReturnType<typeof readDurability>>;

function useStorageReport({ open }: { open: boolean }) {
	const editor = useEditor();
	const [durability, setDurability] = useState<Durability | null>(null);
	const [quota, setQuota] = useState<{ usage: number | null; quota: number | null }>({ usage: null, quota: null });
	const [rows, setRows] = useState<ProjectStorageRow[]>([]);

	const refresh = useCallback(async () => {
		setDurability(await readDurability());
		const status = await readStorageQuotaStatus();
		setQuota({ usage: status.usageBytes, quota: status.quotaBytes });
		if (!open) return;
		const backups = readBackups();
		const projects = editor.project.getSavedProjects();
		const measured = await Promise.all(
			projects.map(async (project) => {
				const sizes = await storageService
					.measureProjectMedia({ projectId: project.id })
					.catch(() => ({ mediaBytes: 0, proxyBytes: 0, missing: [] as string[] }));
				return {
					id: project.id,
					name: project.name,
					updatedAt: new Date(project.updatedAt).getTime(),
					mediaBytes: sizes.mediaBytes,
					proxyBytes: sizes.proxyBytes,
					missing: sizes.missing.length,
					lastBackupAt: backups[project.id] ?? null,
				};
			}),
		);
		setRows(measured.sort((a, b) => b.mediaBytes + b.proxyBytes - (a.mediaBytes + a.proxyBytes)));
	}, [editor.project, open]);

	useEffect(() => {
		void refresh();
	}, [refresh]);

	return { durability, quota, rows, refresh };
}

function LevelBadge({ level }: { level: DurabilityAssessment["level"] }) {
	const t = useT();
	const label = level === "protected" ? t("Protected") : level === "at-risk" ? t("At risk") : t("Unknown");
	return (
		<span
			className={cn(
				"rounded px-1.5 py-0.5 text-xs font-medium",
				level === "protected" && "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
				level === "at-risk" && "bg-amber-500/15 text-amber-700 dark:text-amber-400",
				level === "unknown" && "bg-muted text-muted-foreground",
			)}
			data-testid="storage-level"
		>
			{label}
		</span>
	);
}

/** The header button: storage level at a glance, opening the full report. */
export function StorageStatusButton() {
	const t = useT();
	const [open, setOpen] = useState(false);
	const [durability, setDurability] = useState<Durability | null>(null);

	useEffect(() => {
		void readDurability().then(setDurability);
	}, [open]);

	return (
		<>
			<Button
				variant="outline"
				onClick={() => setOpen(true)}
				title={t("Where your projects are kept and how safe they are")}
				data-testid="storage-status-button"
			>
				{t("Storage")}
				{durability ? <LevelBadge level={durability.level} /> : null}
			</Button>
			<StorageDialog open={open} onOpenChange={setOpen} />
		</>
	);
}

export function StorageDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const { durability, quota, rows, refresh } = useStorageReport({ open });
	const [busy, setBusy] = useState<string | null>(null);

	const protect = async () => {
		const granted = await requestPersistence();
		if (granted) toast.success(t("The browser will keep your projects"));
		else toast.warning(t("The browser said no"), { description: t("Installing the app or visiting often usually changes that.") });
		await refresh();
	};

	const backUp = async (row: ProjectStorageRow) => {
		setBusy(row.id);
		try {
			const { blob, fileName } = await editor.project.exportProjectBundleById({ id: row.id });
			downloadBlob({ blob, filename: fileName });
			toast.success(t("Backed up {name}", { name: row.name }), { description: fileName });
		} catch (error) {
			toast.error(t("Could not back up {name}", { name: row.name }), { description: error instanceof Error ? error.message : String(error) });
		} finally {
			setBusy(null);
			await refresh();
		}
	};

	const clearProxies = async (row: ProjectStorageRow) => {
		setBusy(row.id);
		try {
			const freed = await storageService.clearProjectProxies({ projectId: row.id });
			toast.success(t("Freed {size} of proxies from {name}", { size: formatStorageBytes({ bytes: freed }), name: row.name }));
		} finally {
			setBusy(null);
			await refresh();
		}
	};

	const usedShare = quota.usage !== null && quota.quota ? Math.min(100, (quota.usage / quota.quota) * 100) : null;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-2xl" data-testid="storage-dialog">
				<DialogHeader>
					<DialogTitle className="flex items-center gap-2">
						{t("Storage")} {durability ? <LevelBadge level={durability.level} /> : null}
					</DialogTitle>
					<DialogDescription>
						{t("Projects and media live in this browser only. Back them up as .opencut.zip bundles to keep them safe or move them.")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="flex flex-col gap-4">
					{durability && durability.reasons.length > 0 ? (
						<div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
							<ul className="list-disc pl-4 text-muted-foreground">
								{durability.reasons.map((reason) => (
									<li key={reason}>{reason}</li>
								))}
							</ul>
							{durability.advice.length > 0 ? (
								<ul className="mt-2 list-disc pl-4">
									{durability.advice.map((line) => (
										<li key={line}>{line}</li>
									))}
								</ul>
							) : null}
						</div>
					) : null}

					<div className="flex flex-col gap-1.5">
						<div className="flex justify-between text-sm">
							<span>{t("Used by this site")}</span>
							<span className="tabular-nums text-muted-foreground" data-testid="storage-usage">
								{quota.quota
									? t("{used} of {total}", {
											used: quota.usage !== null ? formatStorageBytes({ bytes: quota.usage }) : t("unknown"),
											total: formatStorageBytes({ bytes: quota.quota }),
										})
									: quota.usage !== null
										? formatStorageBytes({ bytes: quota.usage })
										: t("unknown")}
							</span>
						</div>
						{usedShare !== null ? <Progress value={usedShare} /> : null}
					</div>

					<div className="max-h-72 overflow-y-auto rounded-md border">
						<table className="w-full text-sm">
							<thead className="bg-muted/50 text-xs text-muted-foreground">
								<tr>
									<th className="p-2 text-left font-medium">{t("Project")}</th>
									<th className="p-2 text-right font-medium">{t("Media")}</th>
									<th className="p-2 text-right font-medium">{t("Proxies")}</th>
									<th className="p-2 text-left font-medium">{t("Last backup")}</th>
									<th className="p-2" />
								</tr>
							</thead>
							<tbody>
								{rows.length === 0 ? (
									<tr>
										<td className="p-3 text-muted-foreground" colSpan={5}>
											{t("No projects yet.")}
										</td>
									</tr>
								) : (
									rows.map((row) => {
										const stale = needsBackup({ updatedAt: row.updatedAt, lastBackupAt: row.lastBackupAt });
										return (
											<tr key={row.id} className="border-t" data-testid="storage-row">
												<td className="max-w-48 truncate p-2" title={row.name}>
													{row.name}
													{row.missing > 0 ? (
														<span className="ml-1 text-xs text-destructive">{t("{count} missing", { count: row.missing })}</span>
													) : null}
												</td>
												<td className="p-2 text-right tabular-nums">{formatStorageBytes({ bytes: row.mediaBytes })}</td>
												<td className="p-2 text-right tabular-nums">{formatStorageBytes({ bytes: row.proxyBytes })}</td>
												<td className={cn("p-2", stale && "text-amber-700 dark:text-amber-400")}>
													{stale && row.lastBackupAt
														? t("{age} (changed since)", {
																age: describeBackupAge({ lastBackupAt: row.lastBackupAt, now: Date.now() }),
															})
														: describeBackupAge({ lastBackupAt: row.lastBackupAt, now: Date.now() })}
												</td>
												<td className="flex justify-end gap-1 p-2">
													{row.proxyBytes > 0 ? (
														<Button size="sm" variant="ghost" disabled={busy === row.id} onClick={() => clearProxies(row)}>
															{t("Clear proxies")}
														</Button>
													) : null}
													<Button size="sm" variant={stale ? "secondary" : "ghost"} disabled={busy === row.id} onClick={() => backUp(row)}>
														{t("Back up")}
													</Button>
												</td>
											</tr>
										);
									})
								)}
							</tbody>
						</table>
					</div>
				</DialogBody>
				<DialogFooter>
					{durability && durability.persisted === false ? (
						<Button variant="secondary" onClick={protect}>
							{t("Allow persistent storage")}
						</Button>
					) : null}
					<Button onClick={() => onOpenChange(false)}>{t("Done")}</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

/**
 * A banner on the projects page while the browser may delete projects:
 * Safari's seven-day rule, or storage the browser has not agreed to keep.
 */
export function StorageRiskBanner({ hasProjects }: { hasProjects: boolean }) {
	const t = useT();
	const [durability, setDurability] = useState<Durability | null>(null);
	const [dismissed, setDismissed] = useState(() => {
		try {
			return sessionStorage.getItem("opencut-storage-banner") === "dismissed";
		} catch {
			return false;
		}
	});
	const [open, setOpen] = useState(false);

	useEffect(() => {
		void readDurability().then(setDurability);
	}, []);

	if (!hasProjects || dismissed || !durability || durability.level !== "at-risk") return null;

	const dismiss = () => {
		setDismissed(true);
		try {
			sessionStorage.setItem("opencut-storage-banner", "dismissed");
		} catch {
			// Fine: the banner comes back next visit.
		}
	};

	return (
		<div
			className="mx-8 flex flex-wrap items-center gap-3 rounded-md border border-amber-500/40 bg-amber-500/5 px-4 py-2 text-sm"
			data-testid="storage-risk-banner"
		>
			<span className="flex-1">
				{durability.sevenDayRule
					? t("Safari deletes this site's projects after 7 days without a visit. Install the app or back up your projects.")
					: t("This browser may delete your projects when the disk runs low. Allow persistent storage or back them up.")}
			</span>
			<Button size="sm" variant="secondary" onClick={() => setOpen(true)}>
				{t("Review storage")}
			</Button>
			<Button size="sm" variant="ghost" onClick={dismiss}>
				{t("Not now")}
			</Button>
			<StorageDialog open={open} onOpenChange={setOpen} />
		</div>
	);
}

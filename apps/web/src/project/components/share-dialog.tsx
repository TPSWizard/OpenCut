"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import {
	type RecentShare,
	type ShareResult,
	recentShares,
	rememberPasscode,
	rememberedPasscode,
	shareCurrentProject,
} from "../share";

type Progress = { step: "packing" } | { step: "uploading"; share: number } | null;

/** Uploads the project and its media for 7 days and shows the link to give out. */
export function ShareDialog({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const [passcode, setPasscode] = useState("");
	const [remember, setRemember] = useState(true);
	const [progress, setProgress] = useState<Progress>(null);
	const [result, setResult] = useState<ShareResult | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [recent, setRecent] = useState<RecentShare[]>([]);

	useEffect(() => {
		if (!isOpen) return;
		const saved = rememberedPasscode();
		setPasscode(saved);
		setRemember(true);
		setResult(null);
		setError(null);
		setProgress(null);
		setRecent(recentShares());
	}, [isOpen]);

	const share = async () => {
		if (!passcode || progress) return;
		setError(null);
		setResult(null);
		try {
			const shared = await shareCurrentProject({ editor, passcode, onProgress: setProgress });
			rememberPasscode({ passcode: remember ? passcode : null });
			setResult(shared);
			setRecent(recentShares());
		} catch (failure) {
			setError(failure instanceof Error ? failure.message : String(failure));
		} finally {
			setProgress(null);
		}
	};

	const copy = (link: string) =>
		void navigator.clipboard.writeText(link).then(
			() => toast.success(t("Link copied")),
			() => toast.error(t("The clipboard is not available here")),
		);

	const expiry = (iso: string) => new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

	return (
		<Dialog open={isOpen} onOpenChange={onOpenChange}>
			<DialogContent data-testid="share-dialog">
				<DialogHeader>
					<DialogTitle>{t("Share by link")}</DialogTitle>
					<DialogDescription>
						{t("Uploads this project and its media for 7 days. Anyone with the link can open a copy in ConzaCut; they see what it is before it is added.")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="gap-3">
					{!result && (
						<>
							<Label htmlFor="share-passcode">{t("Sharing passcode")}</Label>
							<Input
								id="share-passcode"
								type="password"
								autoComplete="off"
								value={passcode}
								onChange={(event) => setPasscode(event.target.value)}
								onKeyDown={(event) => {
									if (event.key === "Enter") void share();
								}}
								data-testid="share-passcode"
							/>
							<label className="flex items-center gap-2 text-xs">
								<Checkbox checked={remember} onCheckedChange={(checked) => setRemember(checked === true)} />
								{t("Remember it in this browser")}
							</label>
						</>
					)}
					{progress && (
						<div className="flex flex-col gap-1 text-xs" data-testid="share-progress">
							<span>{progress.step === "packing" ? t("Packing the project…") : t("Uploading… {percent}%", { percent: Math.round(progress.share * 100) })}</span>
							<div className="bg-muted h-1.5 overflow-hidden rounded">
								<div className="bg-foreground h-full" style={{ width: `${progress.step === "packing" ? 3 : Math.round(progress.share * 100)}%` }} />
							</div>
						</div>
					)}
					{error && (
						<p className="text-destructive text-sm" role="alert">
							{error}
						</p>
					)}
					{result && (
						<div className="flex flex-col gap-2" data-testid="share-result">
							<Label>{t("Link")}</Label>
							<div className="flex gap-2">
								<Input readOnly value={result.link} onFocus={(event) => event.currentTarget.select()} data-testid="share-link" />
								<Button onClick={() => copy(result.link)}>{t("Copy")}</Button>
							</div>
							<span className="text-muted-foreground text-xs">{t("Works until {date}.", { date: expiry(result.expiresAt) })}</span>
						</div>
					)}
					{recent.length > 0 && (
						<div className="flex flex-col gap-1">
							<span className="text-muted-foreground text-xs">{t("Shared from this browser")}</span>
							{recent.map((item) => (
								<button
									type="button"
									key={item.link}
									className="hover:bg-accent flex justify-between gap-2 rounded px-1 py-0.5 text-left text-xs"
									onClick={() => copy(item.link)}
									title={t("Copy the link")}
								>
									<span className="truncate">{item.name}</span>
									<span className="text-muted-foreground shrink-0">{t("until {date}", { date: expiry(item.expiresAt) })}</span>
								</button>
							))}
						</div>
					)}
				</DialogBody>
				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{result ? t("Done") : t("Cancel")}
					</Button>
					{!result && (
						<Button disabled={!passcode || progress !== null} onClick={() => void share()} data-testid="share-confirm">
							{t("Share")}
						</Button>
					)}
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

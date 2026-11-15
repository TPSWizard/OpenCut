"use client";

import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { rememberPasscode, rememberedPasscode } from "@/project/share";
import type { StockKind, StockProvider } from "@/stock/stock";
import { assembleBroll, improveQueries } from "../assemble";
import { planShots, type Shot } from "../plan";

const clock = (seconds: number): string => {
	const whole = Math.floor(seconds);
	return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
};

/**
 * "B-roll from a script" in the Stock tab: plan shots from a script (the
 * editor's own search words, or Claude's through the server), adjust the
 * words, then lay the first result for each shot out from the playhead.
 */
export function BrollSection({ provider, kind }: { provider: StockProvider; kind: StockKind }) {
	const t = useT();
	const editor = useEditor();
	const [script, setScript] = useState("");
	const [smart, setSmart] = useState(false);
	const [passcode, setPasscode] = useState(() => rememberedPasscode());
	const [shots, setShots] = useState<Shot[]>([]);
	const [busy, setBusy] = useState<string | null>(null);

	const plan = async () => {
		const planned = planShots({ script });
		if (planned.length === 0) {
			toast.error(t("Write or paste a script first."));
			return;
		}
		setShots(planned);
		if (!smart) return;
		setBusy(t("Asking Claude for search words…"));
		try {
			const improved = await improveQueries({ shots: planned, passcode });
			rememberPasscode({ passcode });
			setShots(improved.shots);
		} catch (error) {
			toast.error(t("Claude's search words are not available"), {
				description: `${error instanceof Error ? error.message : String(error)}. ${t("The editor's own search words are used.")}`,
			});
		} finally {
			setBusy(null);
		}
	};

	const assemble = async () => {
		setBusy(t("Finding pictures…"));
		try {
			const results = await assembleBroll({
				editor,
				shots,
				provider,
				kind,
				onProgress: (done, total) => setBusy(`${t("Finding pictures…")} ${done}/${total}`),
			});
			const placed = results.filter((result) => result.placed).length;
			const missing = results.filter((result) => result.problem);
			if (placed > 0) toast.success(t("{count} B-roll clips added", { count: placed }));
			if (missing.length > 0) {
				toast.error(t("{count} shots found nothing", { count: missing.length }), {
					description: missing.map((result) => `${result.shot.query}: ${result.problem}`).join("\n"),
				});
			}
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(null);
		}
	};

	return (
		<div className="flex flex-col gap-2 border-t pt-3" data-testid="broll-section">
			<p className="text-xs font-medium">{t("B-roll from a script")}</p>
			<p className="text-muted-foreground text-[10px]">
				{t("One shot per sentence, timed for reading aloud; each gets the first search result.")}
			</p>
			<Textarea
				value={script}
				onChange={(event) => setScript(event.target.value)}
				placeholder={t("Paste the script here")}
				className="min-h-20 text-xs"
				data-testid="broll-script"
			/>
			<label className="flex items-center gap-2 text-xs">
				<Checkbox checked={smart} onCheckedChange={(value) => setSmart(value === true)} data-testid="broll-smart" />
				{t("Better search words from Claude (uses the server's key and passcode)")}
			</label>
			{smart && (
				<Input
					type="password"
					value={passcode}
					onChange={(event) => setPasscode(event.target.value)}
					placeholder={t("Server passcode")}
					className="h-8 text-xs"
					data-testid="broll-passcode"
				/>
			)}
			<Button size="sm" variant="outline" disabled={busy !== null || !script.trim()} onClick={() => void plan()} data-testid="broll-plan">
				{t("Plan shots")}
			</Button>
			{shots.length > 0 && (
				<div className="flex flex-col gap-1" data-testid="broll-shots">
					{shots.map((shot, index) => (
						<div key={`${index}-${shot.start}`} className="flex items-center gap-2">
							<span className="text-muted-foreground w-10 shrink-0 text-[10px] tabular-nums">{clock(shot.start)}</span>
							<Input
								value={shot.query}
								title={shot.text}
								onChange={(event) =>
									setShots((current) => current.map((item, at) => (at === index ? { ...item, query: event.target.value } : item)))
								}
								className="h-7 text-xs"
							/>
						</div>
					))}
					<Button size="sm" disabled={busy !== null} onClick={() => void assemble()} data-testid="broll-add">
						{t("Add B-roll at the playhead")}
					</Button>
				</div>
			)}
			{busy && (
				<p className="text-muted-foreground flex items-center gap-2 text-[10px]">
					<Spinner /> {busy}
				</p>
			)}
		</div>
	);
}

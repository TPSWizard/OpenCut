"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Section, SectionContent, SectionHeader, SectionTitle } from "@/components/section";
import { useEditor } from "@/editor/use-editor";
import { rememberPasscode, rememberedPasscode } from "@/project/share";
import { formatSeconds } from "../format";
import { SHORTS_DEFAULTS, type ShortPick } from "../lines";
import { makeShorts, pickShorts, projectLines, type MadeShort } from "../make";

const LENGTHS: { id: string; label: string; min: number; max: number }[] = [
	{ id: "15-30", label: "15–30 s", min: 15, max: 30 },
	{ id: "15-60", label: "15–60 s", min: 15, max: 60 },
	{ id: "30-90", label: "30–90 s", min: 30, max: 90 },
	{ id: "60-180", label: "1–3 min", min: 60, max: 180 },
];

/**
 * Long video to shorts: picks the moments of the open project that stand
 * on their own (Claude through the server, or a local pick) and saves each
 * as its own 9:16 project with its captions.
 */
export function ShortsSection() {
	const editor = useEditor();
	const [count, setCount] = useState(SHORTS_DEFAULTS.count);
	const [length, setLength] = useState("15-60");
	const [passcode, setPasscode] = useState(() => rememberedPasscode());
	const [picks, setPicks] = useState<ShortPick[]>([]);
	const [chosen, setChosen] = useState<Set<number>>(() => new Set());
	const [made, setMade] = useState<MadeShort[]>([]);
	const [busy, setBusy] = useState<string | null>(null);
	const [status, setStatus] = useState<string | null>(null);

	const find = async () => {
		const range = LENGTHS.find((option) => option.id === length) ?? LENGTHS[1];
		setBusy(passcode.trim() ? "Asking Claude…" : "Looking…");
		setStatus(null);
		setMade([]);
		try {
			const lines = projectLines({ editor });
			if (lines.length === 0) throw new Error("No captions with word timings yet: generate a transcript above first.");
			const result = await pickShorts({ lines, options: { count, minSeconds: range.min, maxSeconds: range.max }, passcode });
			if (result.by === "claude") rememberPasscode({ passcode });
			setPicks(result.picks);
			setChosen(new Set(result.picks.map((_, index) => index)));
			setStatus(
				result.picks.length === 0
					? "No run of whole sentences fits that length."
					: `${result.picks.length} moment${result.picks.length === 1 ? "" : "s"} picked ${result.by === "claude" ? "by Claude" : "locally"} from ${lines.length} lines.`,
			);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not pick moments");
		} finally {
			setBusy(null);
		}
	};

	const make = async () => {
		const selected = picks.filter((_, index) => chosen.has(index));
		if (selected.length === 0) return;
		setBusy("Making shorts…");
		try {
			const result = await makeShorts({ editor, picks: selected, onProgress: (done, total) => setBusy(`Making shorts… ${done}/${total}`) });
			setMade(result);
			setStatus(`${result.length} short${result.length === 1 ? "" : "s"} saved as new 9:16 projects; this one is unchanged.`);
		} catch (error) {
			setStatus(error instanceof Error ? error.message : "Could not make the shorts");
		} finally {
			setBusy(null);
		}
	};

	return (
		<Section sectionKey="captions:shorts" collapsible defaultOpen={false}>
			<SectionHeader>
				<SectionTitle>Long video to shorts</SectionTitle>
			</SectionHeader>
			<SectionContent>
				<div className="flex flex-col gap-3" data-testid="shorts-section">
					<p className="text-muted-foreground text-xs">
						Picks the moments that stand on their own from the captions' transcript and saves each as a new 9:16
						project with its captions, centre-cropped (run Auto-reframe there to follow faces).
					</p>
					<div className="grid grid-cols-2 gap-2">
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="shorts-count">Shorts</Label>
							<Select value={String(count)} onValueChange={(value) => setCount(Number(value))}>
								<SelectTrigger id="shorts-count" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{[1, 2, 3, 4, 5, 6, 8, 10].map((value) => (
										<SelectItem key={value} value={String(value)}>
											{value}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="shorts-length">Length</Label>
							<Select value={length} onValueChange={setLength}>
								<SelectTrigger id="shorts-length" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{LENGTHS.map((option) => (
										<SelectItem key={option.id} value={option.id}>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="shorts-passcode">Server passcode (for Claude; leave empty to pick locally)</Label>
						<Input
							id="shorts-passcode"
							type="password"
							value={passcode}
							onChange={(event) => setPasscode(event.target.value)}
							placeholder="Server passcode"
							data-testid="shorts-passcode"
						/>
					</div>
					<Button size="sm" variant="outline" onClick={find} disabled={busy !== null}>
						{busy && picks.length === 0 ? busy : "Find moments"}
					</Button>
					{picks.length > 0 && (
						<ul className="flex flex-col gap-2" data-testid="shorts-picks">
							{picks.map((pick, index) => (
								<li key={`${pick.first}-${pick.last}`} className="bg-muted/40 rounded-md p-2 text-xs">
									<label className="flex items-start gap-2">
										<input
											type="checkbox"
											className="mt-0.5"
											checked={chosen.has(index)}
											onChange={(event) =>
												setChosen((previous) => {
													const next = new Set(previous);
													if (event.target.checked) next.add(index);
													else next.delete(index);
													return next;
												})
											}
										/>
										<span className="flex flex-col gap-0.5">
											<span className="font-medium">{pick.title}</span>
											<span className="text-muted-foreground">
												{formatSeconds(pick.start)}–{formatSeconds(pick.end)} ({Math.round(pick.end - pick.start)} s)
											</span>
											{pick.reason && <span className="text-muted-foreground">{pick.reason}</span>}
										</span>
									</label>
								</li>
							))}
						</ul>
					)}
					{picks.length > 0 && (
						<Button size="sm" onClick={make} disabled={busy !== null || chosen.size === 0}>
							{busy && picks.length > 0 ? busy : `Make ${chosen.size} short${chosen.size === 1 ? "" : "s"}`}
						</Button>
					)}
					{made.length > 0 && (
						<ul className="flex flex-col gap-1 text-xs" data-testid="shorts-made">
							{made.map((short) => (
								<li key={short.projectId}>
									<a className="text-primary underline" href={`/editor/${short.projectId}`}>
										Open “{short.pick.title}”
									</a>
								</li>
							))}
						</ul>
					)}
					{status && (
						<p className="text-muted-foreground text-xs" role="status">
							{status}
						</p>
					)}
				</div>
			</SectionContent>
		</Section>
	);
}

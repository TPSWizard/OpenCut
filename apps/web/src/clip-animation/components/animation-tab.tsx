"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VisualElement } from "@/timeline";
import { cn } from "@/utils/ui";
import { addMediaTime, mediaTimeFromSeconds, mediaTimeToSeconds } from "@/wasm";
import {
	ANIMATION_DURATION_DEFAULT,
	ANIMATION_DURATION_MAX,
	ANIMATION_DURATION_MIN,
	ANIMATION_IN_KINDS,
	ANIMATION_LABELS,
	ANIMATION_LOOP_KINDS,
	ANIMATION_LOOP_PERIOD_DEFAULT,
	ANIMATION_LOOP_PERIOD_MAX,
	ANIMATION_LOOP_PERIOD_MIN,
	ANIMATION_OUT_KINDS,
	TEXT_ONLY_ANIMATIONS,
	type AnimationSlot,
	animationWindows,
	readClipAnimations,
} from "../presets";

const SLOT_KEYS: Record<AnimationSlot, { kind: string; length: string }> = {
	in: { kind: "animIn", length: "animInDuration" },
	out: { kind: "animOut", length: "animOutDuration" },
	loop: { kind: "animLoop", length: "animLoopPeriod" },
};

/**
 * One-click animations for a visual clip: a way in, a way out and a loop,
 * each picked from a grid, with its length. Picking one moves the playhead
 * to where it plays, so the preview shows it.
 */
export function AnimationTab({ element, trackId }: { element: VisualElement; trackId: string }) {
	const t = useT();
	const editor = useEditor();
	const isText = element.type === "text";
	const params = "params" in element ? element.params : {};

	const update = (patch: Record<string, string | number>) => {
		editor.timeline.updateElements({
			updates: [{ trackId, elementId: element.id, patch: { params: { ...params, ...patch } } as never }],
		});
	};

	const showSlot = (slot: AnimationSlot, kind: string) => {
		if (kind === "none") return;
		const animations = readClipAnimations({ ...params, [SLOT_KEYS[slot].kind]: kind });
		if (!animations) return;
		const { outSeconds } = animationWindows({ animations, lengthSeconds: elementSeconds(element) });
		const at = slot === "out" ? Math.max(0, elementSeconds(element) - outSeconds) : 0;
		editor.playback.seek({ time: addMediaTime({ a: element.startTime, b: mediaTimeFromSeconds({ seconds: at }) }) });
	};

	const slots: Array<{
		slot: AnimationSlot;
		title: string;
		kinds: readonly string[];
		lengthLabel: string;
		lengthDefault: number;
		lengthMin: number;
		lengthMax: number;
	}> = [
		{
			slot: "in",
			title: t("In"),
			kinds: ANIMATION_IN_KINDS,
			lengthLabel: t("Length (s)"),
			lengthDefault: ANIMATION_DURATION_DEFAULT,
			lengthMin: ANIMATION_DURATION_MIN,
			lengthMax: ANIMATION_DURATION_MAX,
		},
		{
			slot: "out",
			title: t("Out"),
			kinds: ANIMATION_OUT_KINDS,
			lengthLabel: t("Length (s)"),
			lengthDefault: ANIMATION_DURATION_DEFAULT,
			lengthMin: ANIMATION_DURATION_MIN,
			lengthMax: ANIMATION_DURATION_MAX,
		},
		{
			slot: "loop",
			title: t("Loop"),
			kinds: ANIMATION_LOOP_KINDS,
			lengthLabel: t("Seconds per cycle"),
			lengthDefault: ANIMATION_LOOP_PERIOD_DEFAULT,
			lengthMin: ANIMATION_LOOP_PERIOD_MIN,
			lengthMax: ANIMATION_LOOP_PERIOD_MAX,
		},
	];

	return (
		<div className="flex flex-col gap-5 p-3.5" data-testid="animation-tab">
			<p className="text-muted-foreground text-xs">
				{t("Animations play on top of the clip's keyframes and follow it when you trim it.")}
			</p>
			{slots.map((entry) => {
				const keys = SLOT_KEYS[entry.slot];
				const current = typeof params[keys.kind] === "string" ? (params[keys.kind] as string) : "none";
				const kinds = entry.kinds.filter((kind) => isText || !TEXT_ONLY_ANIMATIONS.has(kind));
				const active = kinds.includes(current) ? current : "none";
				const length = typeof params[keys.length] === "number" ? (params[keys.length] as number) : entry.lengthDefault;
				return (
					<section key={entry.slot} className="flex flex-col gap-2.5" data-testid={`animation-${entry.slot}`}>
						<h3 className="text-sm font-medium">{entry.title}</h3>
						<div className="grid grid-cols-3 gap-1.5">
							{["none", ...kinds].map((kind) => (
								<Button
									key={kind}
									type="button"
									size="sm"
									variant={active === kind ? "default" : "outline"}
									className={cn("h-8 px-1.5 text-xs", active === kind && "pointer-events-none")}
									aria-pressed={active === kind}
									data-testid={`animation-${entry.slot}-${kind}`}
									onClick={() => {
										update({ [keys.kind]: kind });
										showSlot(entry.slot, kind);
									}}
								>
									{kind === "none" ? t("None") : t(ANIMATION_LABELS[kind] ?? kind)}
								</Button>
							))}
						</div>
						{active !== "none" && !(entry.slot === "loop" && active === "kenBurns") ? (
							<label className="flex items-center justify-between gap-3">
								<span className="text-muted-foreground text-xs">{entry.lengthLabel}</span>
								<Input
									type="number"
									min={entry.lengthMin}
									max={entry.lengthMax}
									step={0.1}
									defaultValue={length}
									key={`${entry.slot}:${length}`}
									className="h-8 w-20 text-xs"
									data-testid={`animation-${entry.slot}-length`}
									onBlur={(event) => commitLength(event.target.value)}
									onKeyDown={(event) => {
										if (event.key === "Enter") commitLength((event.target as HTMLInputElement).value);
									}}
								/>
							</label>
						) : null}
						{entry.slot === "loop" && active === "kenBurns" ? (
							<p className="text-muted-foreground text-xs">{t("A slow push in and drift over the whole clip.")}</p>
						) : null}
					</section>
				);

				function commitLength(text: string) {
					const value = Number(text);
					if (!Number.isFinite(value)) return;
					const clamped = Math.min(entry.lengthMax, Math.max(entry.lengthMin, value));
					if (clamped !== length) update({ [keys.length]: clamped });
				}
			})}
		</div>
	);
}

function elementSeconds(element: VisualElement): number {
	return mediaTimeToSeconds({ time: element.duration });
}

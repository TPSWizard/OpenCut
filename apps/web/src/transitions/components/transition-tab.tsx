"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import type { VisualElement } from "@/timeline/types";
import { mediaTimeToSeconds, mediaTimeFromSeconds } from "@/wasm";
import {
	DEFAULT_TRANSITION_DURATION,
	DEFAULT_TRANSITION_TYPE,
	getTransitionCatalogue,
	getTransitionInfo,
	type ElementTransition,
	type TransitionParamInfo,
	type TransitionParamValue,
} from "@/transitions";
import {
	getClipTransition,
	setClipTransition,
	transitionTarget,
	type TransitionSide,
} from "@/transitions/apply";
import { getNextAdjacent } from "@/transitions/window";
import { spaced } from "./assets-view";

const NONE = "__none__";

/**
 * The transitions at a clip's start and end: which one, how long, and its
 * parameters (read from the catalogue, so a new shader needs no UI work).
 * The start is shared with the clip before when one sits directly before
 * it; otherwise the clip comes in from nothing.
 */
export function TransitionTab({
	element,
	trackId,
}: {
	element: VisualElement;
	trackId: string;
}) {
	const t = useT();
	const editor = useEditor();
	const track = useEditor((e) => {
		const { tracks } = e.scenes.getActiveScene();
		return [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId) ?? null;
	});
	if (!track) return null;
	const startTarget = transitionTarget({ track, element, side: "start" });
	const nextClip = getNextAdjacent({ track, element });
	const startNote = startTarget.sharedWith
		? t("Shared with the end of the clip before ({name}), centred on the cut.", {
				name: startTarget.sharedWith.name,
			})
		: t("Nothing comes directly before this clip, so it comes in from nothing over its first moments.");
	const endNote = nextClip
		? t("Plays out of this clip into the next one ({name}), centred on the cut.", { name: nextClip.name })
		: t("Nothing comes directly after this clip, so it goes out to nothing over its last moments.");
	return (
		<div className="flex flex-col gap-5 p-3.5" data-testid="transition-tab">
			{(["start", "end"] as const).map((side) => (
				<TransitionEditor
					key={side}
					side={side}
					title={side === "start" ? t("Start") : t("End")}
					note={side === "start" ? startNote : endNote}
					transition={getClipTransition({ track, element, side })}
					onChange={(next) => setClipTransition({ editor, track, element, side, transition: next })}
				/>
			))}
		</div>
	);
}

function TransitionEditor({
	side,
	title,
	note,
	transition,
	onChange,
}: {
	side: TransitionSide;
	title: string;
	note: string;
	transition: ElementTransition | null;
	onChange: (next: ElementTransition | null) => void;
}) {
	const t = useT();
	const catalogue = getTransitionCatalogue();
	const info = transition ? getTransitionInfo({ name: transition.type }) : null;

	const setType = (type: string) => {
		if (type === NONE) {
			onChange(null);
			return;
		}
		onChange({ type, duration: transition?.duration ?? DEFAULT_TRANSITION_DURATION });
	};

	const setDuration = (seconds: number) => {
		if (!transition || !Number.isFinite(seconds) || seconds <= 0) return;
		onChange({ ...transition, duration: mediaTimeFromSeconds({ seconds }) });
	};

	const setParam = (name: string, value: TransitionParamValue) => {
		if (!transition) return;
		onChange({ ...transition, params: { ...(transition.params ?? {}), [name]: value } });
	};

	return (
		<section className="flex flex-col gap-3" data-testid={`transition-${side}`}>
			<div className="flex items-center justify-between">
				<h3 className="text-sm font-medium">{title}</h3>
				{transition ? (
					<Button variant="ghost" size="sm" onClick={() => onChange(null)}>
						{t("Remove transition")}
					</Button>
				) : null}
			</div>
			<p className="text-muted-foreground text-xs">{note}</p>
			<Field label={t("Transition")}>
				<Select value={transition?.type ?? NONE} onValueChange={setType}>
					<SelectTrigger className="w-full" data-testid={`transition-${side}-type`}>
						<SelectValue placeholder={t("None")} />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value={NONE}>{t("None")}</SelectItem>
						{catalogue.map((entry) => (
							<SelectItem key={entry.name} value={entry.name}>
								{spaced(entry.name)}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</Field>
			{!transition ? (
				<Button variant="outline" size="sm" onClick={() => setType(DEFAULT_TRANSITION_TYPE)}>
					{t("Add {name}", { name: spaced(DEFAULT_TRANSITION_TYPE).toLowerCase() })}
				</Button>
			) : (
				<>
					{info?.description && <p className="text-muted-foreground text-xs">{info.description}</p>}
					<Field label={t("Duration (s)")}>
						<Input
							type="number"
							min={0.05}
							step={0.05}
							defaultValue={mediaTimeToSeconds({ time: transition.duration }).toFixed(2)}
							key={`duration:${transition.duration}`}
							onBlur={(event) => setDuration(Number(event.target.value))}
							onKeyDown={(event) => {
								if (event.key === "Enter") {
									setDuration(Number((event.target as HTMLInputElement).value));
								}
							}}
							className="h-8 text-xs"
						/>
					</Field>
					{info?.params.map((param) => (
						<ParamField
							key={param.name}
							param={param}
							value={transition.params?.[param.name] ?? param.default}
							onChange={(value) => setParam(param.name, value)}
						/>
					))}
				</>
			)}
		</section>
	);
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
	return (
		<label className="flex flex-col gap-1.5">
			<span className="text-muted-foreground text-xs">{label}</span>
			{children}
		</label>
	);
}

function ParamField({
	param,
	value,
	onChange,
}: {
	param: TransitionParamInfo;
	value: TransitionParamValue;
	onChange: (value: TransitionParamValue) => void;
}) {
	const label = param.description ? `${param.name} – ${param.description}` : param.name;
	if (param.kind === "bool") {
		return (
			<div className="flex items-center justify-between gap-2">
				<span className="text-muted-foreground text-xs">{label}</span>
				<Switch checked={Boolean(value)} onCheckedChange={(checked) => onChange(checked)} />
			</div>
		);
	}
	const isInt = param.kind === "int" || param.kind.startsWith("ivec");
	const step = param.step ?? (isInt ? 1 : 0.01);
	const components = Array.isArray(value) ? value : [Number(value)];
	const count = param.kind.startsWith("vec") || param.kind.startsWith("ivec") ? Number(param.kind.slice(-1)) : 1;
	const current = Array.from({ length: count }, (_, index) => components[index] ?? 0);
	const commit = (index: number, raw: string) => {
		const parsed = Number(raw);
		if (!Number.isFinite(parsed)) return;
		const next = current.slice();
		next[index] = isInt ? Math.round(parsed) : parsed;
		onChange(count === 1 ? next[0] : next);
	};
	return (
		<Field label={label}>
			<div className="flex gap-1.5">
				{current.map((component, index) => (
					<Input
						key={`${param.name}:${index}:${component}`}
						type="number"
						step={step}
						min={param.min ?? undefined}
						max={param.max ?? undefined}
						defaultValue={component}
						onBlur={(event) => commit(index, event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								commit(index, (event.target as HTMLInputElement).value);
							}
						}}
						className="h-8 text-xs"
						aria-label={count === 1 ? param.name : `${param.name} ${"xyzw"[index]}`}
					/>
				))}
			</div>
		</Field>
	);
}

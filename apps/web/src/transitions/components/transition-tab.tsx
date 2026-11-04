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
import { setElementTransition } from "@/transitions/apply";
import { spaced } from "./assets-view";

const NONE = "__none__";

/**
 * The transition a clip leads out of: which one, how long, and its
 * parameters (read from the catalogue, so a new shader needs no UI work).
 */
export function TransitionTab({
	element,
	trackId,
}: {
	element: VisualElement;
	trackId: string;
}) {
	const editor = useEditor();
	const catalogue = getTransitionCatalogue();
	const transition = element.transition ?? null;
	const info = transition ? getTransitionInfo({ name: transition.type }) : null;

	const update = (next: ElementTransition | null) =>
		setElementTransition({
			editor,
			trackId,
			elementId: element.id,
			transition: next,
		});

	const setType = (type: string) => {
		if (type === NONE) {
			update(null);
			return;
		}
		update({
			type,
			duration: transition?.duration ?? DEFAULT_TRANSITION_DURATION,
		});
	};

	const setDuration = (seconds: number) => {
		if (!transition || !Number.isFinite(seconds) || seconds <= 0) return;
		update({ ...transition, duration: mediaTimeFromSeconds({ seconds }) });
	};

	const setParam = (name: string, value: TransitionParamValue) => {
		if (!transition) return;
		update({
			...transition,
			params: { ...(transition.params ?? {}), [name]: value },
		});
	};

	return (
		<div className="flex flex-col gap-4 p-3.5">
			<Field label="Transition">
				<Select value={transition?.type ?? NONE} onValueChange={setType}>
					<SelectTrigger className="w-full">
						<SelectValue placeholder="None" />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value={NONE}>None</SelectItem>
						{catalogue.map((entry) => (
							<SelectItem key={entry.name} value={entry.name}>
								{spaced(entry.name)}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</Field>
			{!transition ? (
				<div className="flex flex-col gap-2">
					<p className="text-muted-foreground text-xs">
						A transition plays out of this clip's end into the next clip on the
						track, centred on the cut. With nothing after it, the clip fades out.
					</p>
					<Button
						variant="outline"
						size="sm"
						onClick={() => setType(DEFAULT_TRANSITION_TYPE)}
					>
						Add {spaced(DEFAULT_TRANSITION_TYPE).toLowerCase()}
					</Button>
				</div>
			) : (
				<>
					{info?.description && (
						<p className="text-muted-foreground text-xs">{info.description}</p>
					)}
					<Field label="Duration (s)">
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
					<Button variant="ghost" size="sm" onClick={() => update(null)}>
						Remove transition
					</Button>
				</>
			)}
		</div>
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

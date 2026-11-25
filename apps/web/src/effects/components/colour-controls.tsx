"use client";

import { useEffect, useRef, useState } from "react";
import type { ParamValues } from "@/params";
import { Button } from "@/components/ui/button";
import { useT } from "@/i18n";
import { cn } from "@/utils/ui";
import {
	COLOUR_WHEELS,
	HSL_BANDS,
	colourWheelMove,
	colourWheelPoint,
	colourWheelRgba,
	hslBandValue,
	hslParamKey,
} from "../colour-tools";

const numberOf = (params: ParamValues, key: string) => {
	const value = params[key];
	return typeof value === "number" && Number.isFinite(value) ? value : Number(value) || 0;
};

/**
 * The HSL effect's band picker and its three hue curves (hue, saturation,
 * lightness across the hue circle); only the picked band's rows show
 * under it. The desktop inspector draws the same.
 */
export function HslHeader({ params, band, onBand }: { params: ParamValues; band: number; onBand: (band: number) => void }) {
	const t = useT();
	const width = 240;
	const height = 64;
	const curve = (control: number) => {
		const values = HSL_BANDS.map((_, index) => numberOf(params, hslParamKey(index, control)));
		return Array.from({ length: 73 }, (_, step) => {
			const hue = (step * 360) / 72;
			const value = Math.min(100, Math.max(-100, hslBandValue(values, hue)));
			return `${((step / 72) * width).toFixed(1)},${((1 - (value + 100) / 200) * height).toFixed(1)}`;
		}).join(" ");
	};
	const picked = (HSL_BANDS[band].hue / 360) * width;
	return (
		<div className="flex flex-col gap-2 px-4 pt-3" data-testid="hsl-header">
			<div className="flex flex-wrap items-center gap-1.5">
				{HSL_BANDS.map((entry, index) => (
					<button
						key={entry.key}
						type="button"
						title={t(entry.label)}
						aria-label={t(entry.label)}
						aria-pressed={index === band}
						data-testid={`hsl-band-${entry.key}`}
						className={cn("size-5 rounded-full border-2", index === band ? "border-foreground" : "border-transparent")}
						style={{ background: `hsl(${entry.hue} 100% 50%)` }}
						onClick={() => onBand(index)}
					/>
				))}
				<span className="text-xs">{t(HSL_BANDS[band].label)}</span>
			</div>
			<svg viewBox={`0 0 ${width} ${height + 8}`} className="h-[72px] w-full" preserveAspectRatio="none" data-testid="hsl-curves">
				<defs>
					<linearGradient id="hsl-hues" x1="0" x2="1" y1="0" y2="0">
						{Array.from({ length: 13 }, (_, step) => (
							<stop key={step} offset={step / 12} stopColor={`hsl(${step * 30} 100% 50%)`} />
						))}
					</linearGradient>
				</defs>
				<rect x="0" y={height + 2} width={width} height="6" fill="url(#hsl-hues)" />
				<line x1="0" x2={width} y1={height / 2} y2={height / 2} stroke="currentColor" className="text-muted-foreground" strokeWidth="0.5" />
				<line x1={picked} x2={picked} y1="0" y2={height} stroke="currentColor" className="text-muted-foreground" strokeWidth="0.5" />
				<polyline points={curve(0)} fill="none" stroke="rgb(235,235,235)" strokeWidth="1.5" data-testid="hsl-curve-hue" />
				<polyline points={curve(1)} fill="none" stroke="rgb(250,200,60)" strokeWidth="1.5" data-testid="hsl-curve-saturation" />
				<polyline points={curve(2)} fill="none" stroke="rgb(140,170,255)" strokeWidth="1.5" data-testid="hsl-curve-lightness" />
			</svg>
			<p className="text-muted-foreground text-xs">{t("Curves across the hues: hue (white), saturation (yellow), lightness (blue).")}</p>
		</div>
	);
}

/** Whether an HSL param belongs to a band (only its rows show). */
export function isHslBandParam(key: string, band: number): boolean {
	return key.startsWith(HSL_BANDS[band].key);
}

let wheelPicture: ImageData | null = null;
const WHEEL_PIXELS = 168;

function ColourWheel({
	label,
	offsets,
	onPreview,
	onCommit,
	onCentre,
	testId,
}: {
	label: string;
	offsets: [number, number, number];
	onPreview: (next: [number, number, number]) => void;
	onCommit: () => void;
	onCentre: () => void;
	testId: string;
}) {
	const t = useT();
	const canvasRef = useRef<HTMLCanvasElement>(null);
	const base = useRef<[number, number, number] | null>(null);
	const [drag, setDrag] = useState<{ x: number; y: number } | null>(null);

	const finish = () => {
		if (!base.current) return;
		base.current = null;
		setDrag(null);
		onCommit();
	};
	const finishRef = useRef(finish);
	finishRef.current = finish;

	// A release anywhere ends the drag, even one the wheel did not see.
	useEffect(() => {
		if (!drag) return;
		const end = () => finishRef.current();
		window.addEventListener("pointerup", end);
		window.addEventListener("mouseup", end);
		return () => {
			window.removeEventListener("pointerup", end);
			window.removeEventListener("mouseup", end);
		};
	}, [drag]);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;
		wheelPicture ??= new ImageData(new Uint8ClampedArray(colourWheelRgba(WHEEL_PIXELS)), WHEEL_PIXELS, WHEEL_PIXELS);
		canvas.getContext("2d")?.putImageData(wheelPicture, 0, 0);
	}, []);

	const at = (event: React.PointerEvent<HTMLDivElement>) => {
		const box = event.currentTarget.getBoundingClientRect();
		const half = box.width / 2;
		return { x: (event.clientX - box.left - half) / half, y: (half - (event.clientY - box.top)) / half };
	};
	const point = drag ?? colourWheelPoint(offsets);
	const magnitude = Math.max(1, Math.hypot(point.x, point.y));
	const dot = { left: 50 + (point.x / magnitude) * 50, top: 50 - (point.y / magnitude) * 50 };

	return (
		<div className="flex flex-col items-center gap-1">
			<div
				className="relative size-[84px] cursor-pointer touch-none"
				data-testid={testId}
				onPointerDown={(event) => {
					event.currentTarget.setPointerCapture(event.pointerId);
					base.current = offsets;
					const next = at(event);
					setDrag(next);
					onPreview(colourWheelMove(offsets, next.x, next.y));
				}}
				onPointerMove={(event) => {
					if (!base.current) return;
					const next = at(event);
					setDrag(next);
					onPreview(colourWheelMove(base.current, next.x, next.y));
				}}
				onPointerUp={finish}
				onPointerCancel={finish}
				onLostPointerCapture={finish}
			>
				<canvas ref={canvasRef} width={WHEEL_PIXELS} height={WHEEL_PIXELS} className="size-full" />
				<div
					className="pointer-events-none absolute size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white bg-black/40"
					style={{ left: `${dot.left}%`, top: `${dot.top}%` }}
				/>
			</div>
			<span className="text-muted-foreground text-xs">{label}</span>
			<Button variant="text" size="sm" className="h-5 px-1 text-xs" onClick={onCentre}>
				{t("Centre")}
			</Button>
		</div>
	);
}

/**
 * Colour wheels over lift, gamma and gain: drag a dot to tint the
 * shadows, midtones or highlights (the same mapping as the desktop's).
 */
export function ColourWheels({
	params,
	onPreview,
	onCommit,
}: {
	params: ParamValues;
	onPreview: (values: Record<string, number>) => void;
	onCommit: () => void;
}) {
	const t = useT();
	const round = (value: number) => Math.round(value * 1000) / 1000;
	const write = (keys: readonly string[], next: readonly number[]) =>
		onPreview(Object.fromEntries(keys.map((key, channel) => [key, round(next[channel])])));
	return (
		<div className="flex flex-col gap-1.5 px-4 pt-3" data-testid="colour-wheels">
			<div className="flex flex-wrap justify-around gap-2">
				{COLOUR_WHEELS.map((wheel) => {
					const offsets = wheel.keys.map((key) => numberOf(params, key)) as [number, number, number];
					return (
						<ColourWheel
							key={wheel.label}
							label={t(wheel.label)}
							offsets={offsets}
							testId={`colour-wheel-${wheel.label.toLowerCase()}`}
							onPreview={(next) => write(wheel.keys, next)}
							onCommit={onCommit}
							onCentre={() => {
								write(wheel.keys, colourWheelMove(offsets, 0, 0));
								onCommit();
							}}
						/>
					);
				})}
			</div>
			<p className="text-muted-foreground text-xs">
				{t("Drag a dot to tint the shadows, midtones or highlights; the rows below show the numbers.")}
			</p>
		</div>
	);
}

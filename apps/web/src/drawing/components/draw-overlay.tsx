"use client";

import { useEffect, useRef } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { cn } from "@/utils/ui";
import {
	DRAW_COLOURS,
	DRAW_TOOL_OPACITY,
	DRAW_TOOLS,
	DRAW_TOOL_WIDTHS,
	addDrawingStroke,
	isDrawing,
	removeLastStroke,
	secondsSince,
	type StrokePoint,
	useDrawStore,
} from "../strokes";

/**
 * Draw on the preview (row 90): while Draw is on, the pointer draws a
 * stroke over the frame (shown here as it is drawn), added on release to
 * the selected drawing or a new one at the playhead.
 */
export function DrawOverlay({ left, top, width, height }: { left: number; top: number; width: number; height: number }) {
	const editor = useEditor();
	const brush = useDrawStore((state) => state.brush);
	const canvasRef = useRef<HTMLCanvasElement>(null);
	const stroke = useRef<{ start: number; points: StrokePoint[] } | null>(null);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (canvas) canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height);
	}, [width, height, brush]);

	if (!brush || width <= 0 || height <= 0) return null;

	const pointAt = (event: React.PointerEvent<HTMLCanvasElement>, start: number): StrokePoint => {
		const box = event.currentTarget.getBoundingClientRect();
		const x = (event.clientX - box.left) / box.width;
		const y = (event.clientY - box.top) / box.height;
		// A mouse reports 0 or 0.5; it draws at mid weight.
		const pressure = event.pointerType === "pen" && event.pressure > 0 ? event.pressure : 0.5;
		return [x, y, pressure, secondsSince(start, event.timeStamp)];
	};
	const paint = () => {
		const canvas = canvasRef.current;
		const context = canvas?.getContext("2d");
		const points = stroke.current?.points;
		if (!canvas || !context || !points) return;
		context.clearRect(0, 0, canvas.width, canvas.height);
		context.strokeStyle = brush.color;
		context.globalAlpha = DRAW_TOOL_OPACITY[brush.tool];
		context.lineCap = "round";
		context.lineJoin = "round";
		context.lineWidth = Math.max(1, brush.width * canvas.height);
		context.beginPath();
		points.forEach(([x, y], index) => {
			if (index === 0) context.moveTo(x * canvas.width, y * canvas.height);
			else context.lineTo(x * canvas.width, y * canvas.height);
		});
		if (points.length === 1) context.lineTo(points[0][0] * canvas.width + 0.01, points[0][1] * canvas.height);
		context.stroke();
	};
	const finish = () => {
		const done = stroke.current;
		stroke.current = null;
		if (!done) return;
		addDrawingStroke({ editor, stroke: { ...brush, points: done.points } });
		canvasRef.current?.getContext("2d")?.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
	};

	return (
		<canvas
			ref={canvasRef}
			width={Math.max(1, Math.round(width))}
			height={Math.max(1, Math.round(height))}
			className="absolute z-20 cursor-crosshair touch-none"
			style={{ left, top, width, height }}
			data-testid="draw-overlay"
			onPointerDown={(event) => {
				event.currentTarget.setPointerCapture(event.pointerId);
				const start = event.timeStamp;
				stroke.current = { start, points: [pointAt(event, start)] };
				paint();
			}}
			onPointerMove={(event) => {
				if (!stroke.current) return;
				const point = pointAt(event, stroke.current.start);
				const last = stroke.current.points.at(-1);
				if (last && Math.hypot((point[0] - last[0]) * width, (point[1] - last[1]) * height) < 0.5) return;
				stroke.current.points.push(point);
				paint();
			}}
			onPointerUp={finish}
			onPointerCancel={finish}
		/>
	);
}

/** The brush bar shown over the preview while Draw is on. */
export function DrawBar() {
	const t = useT();
	const editor = useEditor();
	const brush = useDrawStore((state) => state.brush);
	const setBrush = useDrawStore((state) => state.setBrush);
	const selection = useEditor((e) => e.selection.getSelectedElements());
	const tracks = useEditor((e) => e.scenes.getActiveSceneOrNull()?.tracks);
	if (!brush) return null;
	const ref = selection.length === 1 ? selection[0] : null;
	const all = tracks ? [...tracks.overlay, tracks.main] : [];
	const selected = ref ? all.find((track) => track.id === ref.trackId)?.elements.find((element) => element.id === ref.elementId) : undefined;
	const labels: Record<string, string> = { pen: t("Pen"), marker: t("Marker"), highlighter: t("Highlighter") };
	const sizes: [string, number][] = [
		["S", 0.5],
		["M", 1],
		["L", 2],
	];
	return (
		<div className="bg-background/90 absolute top-2 left-2 z-30 flex flex-wrap items-center gap-1 rounded-md border p-1 text-xs" data-testid="draw-bar">
			{DRAW_TOOLS.map((tool) => (
				<Button
					key={tool}
					size="sm"
					variant={brush.tool === tool ? "secondary" : "text"}
					className="h-6 px-2 text-xs"
					data-testid={`draw-tool-${tool}`}
					onClick={() => setBrush({ ...brush, tool, width: DRAW_TOOL_WIDTHS[tool] })}
				>
					{labels[tool]}
				</Button>
			))}
			{DRAW_COLOURS.map((colour) => (
				<button
					key={colour}
					type="button"
					aria-label={colour}
					data-testid={`draw-colour-${colour.slice(1)}`}
					className={cn("size-4 rounded-full border-2", brush.color === colour ? "border-foreground" : "border-border")}
					style={{ background: colour }}
					onClick={() => setBrush({ ...brush, color: colour })}
				/>
			))}
			{sizes.map(([label, factor]) => {
				const width = DRAW_TOOL_WIDTHS[brush.tool] * factor;
				return (
					<Button
						key={label}
						size="sm"
						variant={Math.abs(brush.width - width) < 1e-9 ? "secondary" : "text"}
						className="h-6 w-6 p-0 text-xs"
						onClick={() => setBrush({ ...brush, width })}
					>
						{label}
					</Button>
				);
			})}
			<Button
				size="sm"
				variant="text"
				className="h-6 px-2 text-xs"
				disabled={!isDrawing(selected) || !ref}
				data-testid="draw-remove-last"
				onClick={() => {
					if (ref) removeLastStroke({ editor, trackId: ref.trackId, elementId: ref.elementId });
				}}
			>
				{t("Remove last stroke")}
			</Button>
			<Button size="sm" variant="text" className="h-6 px-2 text-xs" onClick={() => setBrush(null)}>
				{t("Done")}
			</Button>
		</div>
	);
}

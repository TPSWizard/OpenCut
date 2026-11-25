"use client";

import { useEffect, useRef } from "react";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { useScopeStore, type VideoScopeKind } from "../capture";

/**
 * The video scope beside the preview: pick a scope and see it drawn from
 * each preview frame, with the luma levels and clipped share under it.
 */
export function ScopesPanel() {
	const t = useT();
	const editor = useEditor();
	const scope = useScopeStore((state) => state.scope);
	const result = useScopeStore((state) => state.result);
	const setScope = useScopeStore((state) => state.setScope);
	const canvasRef = useRef<HTMLCanvasElement>(null);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas || !result) return;
		canvas.width = result.width;
		canvas.height = result.height;
		canvas.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(result.rgba), result.width, result.height), 0, 0);
	}, [result]);

	if (!scope) return null;
	const choices: { id: VideoScopeKind; label: string }[] = [
		{ id: "waveform", label: t("Waveform") },
		{ id: "parade", label: t("RGB parade") },
		{ id: "vectorscope", label: t("Vectorscope") },
		{ id: "histogram", label: t("Histogram") },
	];
	const percent = (share: number) => (share * 100).toFixed(1);

	return (
		<div className="flex w-72 shrink-0 flex-col gap-2 border-l p-2" data-testid="scopes-panel">
			<div className="flex flex-wrap gap-1">
				{choices.map((choice) => (
					<Button
						key={choice.id}
						size="sm"
						variant={choice.id === scope ? "secondary" : "text"}
						className="h-6 px-2 text-xs"
						data-testid={`scope-${choice.id}`}
						onClick={() => {
							setScope(choice.id);
							editor.renderer.requestRedraw();
						}}
					>
						{choice.label}
					</Button>
				))}
			</div>
			<canvas
				ref={canvasRef}
				className="h-48 w-full rounded-sm bg-black object-contain"
				data-testid="scope-canvas"
				data-scope={result?.scope ?? ""}
			/>
			{result ? (
				<div className="text-muted-foreground flex flex-col gap-0.5 text-xs" data-testid="scope-stats">
					<span>{t("Luma 1%: {black} · 99%: {white} (0–255)", { black: result.blackLevel, white: result.whiteLevel })}</span>
					<span>
						{t("A channel at 0: {black}% · at 255: {white}%", {
							black: percent(result.atBlack),
							white: percent(result.atWhite),
						})}
					</span>
				</div>
			) : (
				<span className="text-muted-foreground text-xs">{t("Drawing…")}</span>
			)}
		</div>
	);
}

"use client";

import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { decodeLutParam, encodeLutParam, parseLutFile } from "../parse";

/**
 * The LUT effect's table: shows the loaded table's name and size, loads a
 * .cube, .3dl or HALD image into the parameter, and clears it.
 */
export function LutPicker({ value, onChange }: { value: string; onChange: (encoded: string) => void }) {
	const inputRef = useRef<HTMLInputElement>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const decoded = value ? decodeLutParam({ param: value }) : null;

	const load = async (file: File | undefined) => {
		if (!file) return;
		setBusy(true);
		setError(null);
		try {
			const { name, table } = await parseLutFile({ file });
			onChange(encodeLutParam({ name, table }));
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : "Not a LUT file");
		} finally {
			setBusy(false);
			if (inputRef.current) inputRef.current.value = "";
		}
	};

	return (
		<div className="flex flex-col gap-1.5" data-testid="lut-picker">
			<input
				ref={inputRef}
				type="file"
				accept=".cube,.3dl,.png,.jpg,.jpeg,.webp"
				className="hidden"
				onChange={(event) => void load(event.target.files?.[0])}
			/>
			<div className="flex items-center gap-2">
				<span className="text-muted-foreground min-w-0 flex-1 truncate text-xs">
					{decoded ? `${decoded.name} (${decoded.table.size}³)` : value ? "Unreadable table" : "No table loaded"}
				</span>
				<Button size="sm" variant="outline" disabled={busy} onClick={() => inputRef.current?.click()}>
					{busy ? "Loading…" : "Load…"}
				</Button>
				{value && (
					<Button size="sm" variant="ghost" disabled={busy} onClick={() => onChange("")}>
						Clear
					</Button>
				)}
			</div>
			{error && <p className="text-destructive text-xs">{error}</p>}
		</div>
	);
}

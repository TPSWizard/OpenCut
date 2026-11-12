"use client";

import { useRef } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { useEditor } from "@/editor/use-editor";
import type { MediaAssetData } from "@/services/storage/types";
import { formatStorageBytes } from "@/services/storage/quota";

/**
 * Media whose record is stored but whose file is gone (the browser cleared
 * part of the site's storage). Picking the original files puts them back
 * under the same ids, so the clips that use them play again.
 */
export function MissingMediaSection() {
	const editor = useEditor();
	const missing = useEditor((e) => e.media.getMissingAssets());
	const projectId = useEditor((e) => e.project.getActive()?.metadata.id);
	const bulkInput = useRef<HTMLInputElement>(null);
	const singleInput = useRef<HTMLInputElement>(null);
	const singleTarget = useRef<MediaAssetData | null>(null);

	if (!projectId || missing.length === 0) return null;

	const relinkMany = async (files: File[]) => {
		const { relinked, unmatched, failed } = await editor.media.relinkByFiles({ projectId, files });
		if (relinked.length > 0) toast.success(`Relinked ${relinked.length} file(s)`);
		if (failed.length > 0) toast.error("Some files could not be relinked", { description: failed.join("; ") });
		if (relinked.length === 0 && failed.length === 0) {
			toast.warning("No picked file matches a missing name", {
				description: unmatched.length > 0 ? `Picked: ${unmatched.join(", ")}` : undefined,
			});
		}
	};

	const relinkOne = async (file: File) => {
		const target = singleTarget.current;
		if (!target) return;
		try {
			await editor.media.relinkMedia({ projectId, mediaId: target.id, file });
			toast.success(`${target.name} relinked to ${file.name}`);
		} catch (error) {
			toast.error(`Could not relink ${target.name}`, { description: error instanceof Error ? error.message : String(error) });
		}
	};

	return (
		<div className="mx-3 mb-2 rounded-md border border-destructive/40 bg-destructive/5 p-2 text-xs" data-testid="missing-media">
			<input
				ref={bulkInput}
				type="file"
				multiple
				accept="image/*,video/*,audio/*"
				className="hidden"
				onChange={(event) => {
					const files = Array.from(event.target.files ?? []);
					event.target.value = "";
					if (files.length > 0) void relinkMany(files);
				}}
			/>
			<input
				ref={singleInput}
				type="file"
				accept="image/*,video/*,audio/*"
				className="hidden"
				onChange={(event) => {
					const file = event.target.files?.[0];
					event.target.value = "";
					if (file) void relinkOne(file);
				}}
			/>
			<div className="mb-1 flex items-center gap-2">
				<span className="flex-1 font-medium">
					{missing.length} missing file{missing.length === 1 ? "" : "s"}
				</span>
				<Button size="sm" variant="secondary" onClick={() => bulkInput.current?.click()}>
					Relink files…
				</Button>
			</div>
			<p className="mb-1 text-muted-foreground">
				The browser no longer has these files. Pick the originals (matched by name) to bring their clips back.
			</p>
			<ul className="flex flex-col gap-0.5">
				{missing.map((item) => (
					<li key={item.id} className="flex items-center gap-2">
						<span className="flex-1 truncate" title={item.name}>
							{item.name}
							<span className="ml-1 text-muted-foreground">
								{item.type} · {formatStorageBytes({ bytes: item.size })}
							</span>
						</span>
						<Button
							size="sm"
							variant="ghost"
							onClick={() => {
								singleTarget.current = item;
								singleInput.current?.click();
							}}
						>
							Relink
						</Button>
						<Button
							size="sm"
							variant="ghost"
							title="Forget this file; clips that used it stay empty"
							onClick={() => void editor.media.forgetMissing({ projectId, mediaId: item.id })}
						>
							Forget
						</Button>
					</li>
				))}
			</ul>
		</div>
	);
}

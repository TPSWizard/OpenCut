"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { saveProjectAsTemplate } from "../templates";

/** Saves the open project, with its media, as one of your templates in this browser. */
export function SaveTemplateDialog({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const projectName = useEditor((e) => e.project.getActive()?.metadata.name ?? "");
	const [name, setName] = useState(projectName);
	const [saving, setSaving] = useState(false);

	useEffect(() => {
		if (isOpen) setName(projectName);
	}, [isOpen, projectName]);

	const save = async () => {
		if (saving) return;
		setSaving(true);
		try {
			const template = await saveProjectAsTemplate({ editor, name });
			toast.success(t("Saved as a template"), {
				description: t("{name} is under Templates on the projects page.", { name: template.name }),
			});
			onOpenChange(false);
		} catch (error) {
			toast.error(t("The template could not be saved"), {
				description: error instanceof Error ? error.message : String(error),
			});
		} finally {
			setSaving(false);
		}
	};

	return (
		<Dialog open={isOpen} onOpenChange={onOpenChange}>
			<DialogContent data-testid="save-template-dialog">
				<DialogHeader>
					<DialogTitle>{t("Save as template")}</DialogTitle>
					<DialogDescription>
						{t("New projects can start from this one: its timeline, settings and media are copied. It stays in this browser.")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="gap-3">
					<Label>{t("Template name")}</Label>
					<Input
						value={name}
						onChange={(event) => setName(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								event.preventDefault();
								void save();
							}
						}}
						data-testid="save-template-name"
					/>
				</DialogBody>
				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{t("Cancel")}
					</Button>
					<Button disabled={saving || !name.trim()} onClick={() => void save()} data-testid="save-template-confirm">
						{saving ? t("Saving…") : t("Save template")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

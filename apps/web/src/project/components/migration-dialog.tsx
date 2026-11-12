"use client";

import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { useEditor } from "@/editor/use-editor";
import { Loader2 } from "lucide-react";
import { useT } from "@/i18n";

export function MigrationDialog() {
	const t = useT();
	const editor = useEditor();
	const migrationState = editor.project.getMigrationState();

	if (!migrationState.isMigrating) return null;

	const title = migrationState.projectName
		? t("Updating project")
		: t("Updating projects");
	const description = migrationState.projectName
		? t("Upgrading \"{name}\" from v{from} to v{to}", { name: migrationState.projectName, from: String(migrationState.fromVersion), to: String(migrationState.toVersion) })
		: t("Upgrading projects from v{from} to v{to}", { from: String(migrationState.fromVersion), to: String(migrationState.toVersion) });

	return (
		<Dialog open={true}>
			<DialogContent
				className="sm:max-w-md"
				onPointerDownOutside={(event) => event.preventDefault()}
				onEscapeKeyDown={(event) => event.preventDefault()}
			>
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription>{description}</DialogDescription>
				</DialogHeader>

				<div className="flex items-center justify-center py-4">
					<Loader2 className="text-muted-foreground size-8 animate-spin" />
				</div>
			</DialogContent>
		</Dialog>
	);
}

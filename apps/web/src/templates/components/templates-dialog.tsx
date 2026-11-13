"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { useRouter } from "@/compat/navigation";
import { useEditor } from "@/editor/use-editor";
import { loadFonts } from "@/fonts/google-fonts";
import { useT } from "@/i18n";
import {
	createProjectFromTemplate,
	deleteMyTemplate,
	listMyTemplates,
	loadStarterTemplates,
	type TemplateInfo,
} from "../templates";

/** A drawn stand-in for the template's first frame: its colours, title and font. */
function TemplateCard({ template }: { template: TemplateInfo }) {
	const { preview } = template;
	const tall = template.height > template.width;
	if (template.thumbnail) {
		return (
			<div className="bg-muted flex aspect-video items-center justify-center overflow-hidden rounded-md">
				<img src={template.thumbnail} alt="" className="h-full w-full object-contain" />
			</div>
		);
	}
	return (
		<div className="bg-muted flex aspect-video items-center justify-center overflow-hidden rounded-md">
			<div
				className="relative flex h-full items-center justify-center overflow-hidden"
				style={{
					aspectRatio: `${template.width} / ${template.height}`,
					background: preview.background,
					maxWidth: "100%",
				}}
			>
				<span
					className="px-[6%] text-center leading-tight"
					style={{
						color: preview.color,
						fontFamily: preview.font ? `"${preview.font}", sans-serif` : undefined,
						fontStyle: preview.italic ? "italic" : undefined,
						fontWeight: preview.box ? 700 : undefined,
						fontSize: tall ? "0.7rem" : "0.95rem",
						background: preview.box,
						borderRadius: preview.box ? 4 : undefined,
						padding: preview.box ? "1px 6px" : undefined,
						...(preview.lowerThird ? { position: "absolute", left: "10%", bottom: "14%", fontSize: "0.7rem" } : {}),
					}}
				>
					{preview.title}
				</span>
			</div>
		</div>
	);
}

/** Starter templates and yours; picking one makes a new project from it and opens it. */
export function TemplatesDialog({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
	const t = useT();
	const editor = useEditor();
	const router = useRouter();
	const [starter, setStarter] = useState<TemplateInfo[] | null>(null);
	const [mine, setMine] = useState<TemplateInfo[]>([]);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState<string | null>(null);

	useEffect(() => {
		if (!isOpen) return;
		let cancelled = false;
		setError(null);
		loadStarterTemplates().then(
			(templates) => {
				if (cancelled) return;
				setStarter(templates);
				void loadFonts({ families: [...new Set(templates.flatMap((template) => (template.preview.font ? [template.preview.font] : [])))] }).catch(() => undefined);
			},
			(failure) => !cancelled && setError(failure instanceof Error ? failure.message : String(failure)),
		);
		listMyTemplates().then(
			(templates) => !cancelled && setMine(templates),
			() => undefined,
		);
		return () => {
			cancelled = true;
		};
	}, [isOpen]);

	const use = async (template: TemplateInfo) => {
		setBusy(template.id);
		try {
			const projectId = await createProjectFromTemplate({ editor, template });
			onOpenChange(false);
			router.push(`/editor/${projectId}`);
		} catch (failure) {
			toast.error(t("The template could not be used"), { description: failure instanceof Error ? failure.message : String(failure) });
		} finally {
			setBusy(null);
		}
	};

	const remove = async (template: TemplateInfo) => {
		await deleteMyTemplate({ id: template.id });
		setMine((current) => current.filter((candidate) => candidate.id !== template.id));
		toast.success(t("Template deleted"));
	};

	const grid = (templates: TemplateInfo[]) => (
		<div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(170px, 1fr))" }}>
			{templates.map((template) => (
				<div key={`${template.source}:${template.id}`} className="relative flex flex-col gap-1">
					<button
						type="button"
						disabled={busy !== null}
						onClick={() => void use(template)}
						className="hover:ring-foreground/30 flex flex-col gap-1 rounded-md text-left hover:ring-2"
						title={template.description}
						data-testid="template-card"
					>
						<TemplateCard template={template} />
						<span className="truncate px-0.5 text-sm font-medium">{template.name}</span>
						<span className="text-muted-foreground px-0.5 text-xs">
							{template.width}×{template.height} · {Math.round(template.duration)} s
						</span>
					</button>
					{template.source === "mine" && (
						<Button size="sm" variant="ghost" className="h-6 self-start px-1 text-xs" onClick={() => void remove(template)}>
							{t("Delete")}
						</Button>
					)}
					{busy === template.id && (
						<span className="bg-background/70 absolute inset-0 flex items-center justify-center rounded-md">
							<Spinner />
						</span>
					)}
				</div>
			))}
		</div>
	);

	return (
		<Dialog open={isOpen} onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[85vh] max-w-3xl flex-col p-0" data-testid="templates-dialog">
				<DialogHeader>
					<DialogTitle>{t("Start from a template")}</DialogTitle>
					<DialogDescription>
						{t("Pick one to make a new project from it; change the words, add your clips. Save any project as a template from its menu.")}
					</DialogDescription>
				</DialogHeader>
				<DialogBody className="scrollbar-thin flex grow flex-col gap-5 overflow-y-auto">
					{mine.length > 0 && (
						<section className="flex flex-col gap-2">
							<h3 className="text-sm font-medium">{t("Your templates")}</h3>
							{grid(mine)}
						</section>
					)}
					<section className="flex flex-col gap-2">
						<h3 className="text-sm font-medium">{t("Starter templates")}</h3>
						{error && <p className="text-destructive text-sm">{error}</p>}
						{!starter && !error && (
							<div className="flex justify-center py-6">
								<Spinner />
							</div>
						)}
						{starter && grid(starter)}
					</section>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}

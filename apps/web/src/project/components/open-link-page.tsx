"use client";

import { useEffect, useRef, useState } from "react";
import Link from "@/compat/link";
import { useRouter } from "@/compat/navigation";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { mediaTimeToSeconds } from "@/wasm";
import { readProjectBundle } from "../bundle";
import { bundleUrlFromLink, fetchLinkedBundle, MAX_LINKED_BUNDLE_BYTES } from "../open-link";

/** What the page knows about the bundle before it is added. */
interface Summary {
	name: string;
	width: number;
	height: number;
	duration: number;
	mediaCount: number;
	bytes: number;
}

type Step =
	| { kind: "invalid" }
	| { kind: "confirm"; url: string; host: string }
	| { kind: "waiting" }
	| { kind: "downloading"; received: number; total: number | null }
	| { kind: "ready"; blob: Blob; summary: Summary; from: string }
	| { kind: "importing" }
	| { kind: "error"; message: string };

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(0)} KB`;
	if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
	return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

async function summarize(blob: Blob): Promise<Summary> {
	const { project, media } = await readProjectBundle({ file: blob });
	return {
		name: project.metadata.name,
		width: project.settings.canvasSize.width,
		height: project.settings.canvasSize.height,
		// Metadata keeps the duration in timeline ticks.
		duration: project.metadata.duration === undefined ? 0 : mediaTimeToSeconds({ time: project.metadata.duration }),
		mediaCount: media.length,
		bytes: blob.size,
	};
}

/**
 * `/open?bundle=<url>` downloads a project bundle (after asking) and adds
 * it to your projects; `/open?handoff=1` receives one from the page that
 * opened this one (`window.open`), which posts
 * `{ type: "conzacut:bundle", bundle: Blob | ArrayBuffer }` once this page
 * posts `{ type: "conzacut:ready" }`. Nothing is added until you confirm.
 */
export function OpenLinkPage() {
	const t = useT();
	const editor = useEditor();
	const router = useRouter();
	const [step, setStep] = useState<Step>({ kind: "waiting" });
	const handled = useRef(false);

	useEffect(() => {
		if (handled.current) return;
		handled.current = true;
		const params = new URLSearchParams(window.location.search);
		if (params.get("handoff") === "1") {
			const opener = window.opener as Window | null;
			if (!opener) {
				setStep({ kind: "error", message: t("Nothing was handed over: open this page from the app that made the project.") });
				return;
			}
			const onMessage = (event: MessageEvent) => {
				if (event.source !== opener) return;
				const data = event.data as { type?: unknown; bundle?: unknown } | null;
				if (!data || data.type !== "conzacut:bundle") return;
				window.removeEventListener("message", onMessage);
				const bundle = data.bundle;
				const blob =
					bundle instanceof Blob
						? bundle
						: bundle instanceof ArrayBuffer || ArrayBuffer.isView(bundle)
							? new Blob([bundle as BlobPart], { type: "application/zip" })
							: null;
				if (!blob || blob.size > MAX_LINKED_BUNDLE_BYTES) {
					setStep({ kind: "error", message: t("What was handed over is not a project bundle.") });
					return;
				}
				summarize(blob).then(
					(summary) => setStep({ kind: "ready", blob, summary, from: event.origin }),
					(error) => setStep({ kind: "error", message: error instanceof Error ? error.message : String(error) }),
				);
			};
			window.addEventListener("message", onMessage);
			// Only a ready signal goes out; nothing about this app or its projects.
			opener.postMessage({ type: "conzacut:ready" }, "*");
			return;
		}
		const url = bundleUrlFromLink(window.location.href);
		if (!url) {
			setStep({ kind: "invalid" });
			return;
		}
		const host = url.split("://")[1]?.split(/[/?#]/)[0] ?? url;
		setStep({ kind: "confirm", url, host });
	}, [t]);

	const download = async (url: string, host: string) => {
		setStep({ kind: "downloading", received: 0, total: null });
		try {
			const blob = await fetchLinkedBundle({ url, onProgress: ({ received, total }) => setStep({ kind: "downloading", received, total }) });
			setStep({ kind: "ready", blob, summary: await summarize(blob), from: host });
		} catch (error) {
			setStep({ kind: "error", message: error instanceof Error ? error.message : String(error) });
		}
	};

	const add = async (blob: Blob) => {
		setStep({ kind: "importing" });
		try {
			const projectId = await editor.project.importProjectBundle({ file: blob });
			router.push(`/editor/${projectId}`);
		} catch (error) {
			setStep({ kind: "error", message: error instanceof Error ? error.message : String(error) });
		}
	};

	return (
		<div className="bg-background flex min-h-svh items-center justify-center p-6">
			<div className="flex w-full max-w-md flex-col gap-4 rounded-lg border p-6" data-testid="open-link">
				<h1 className="text-lg font-semibold">{t("Open in ConzaCut")}</h1>
				{step.kind === "waiting" && (
					<div className="text-muted-foreground flex items-center gap-2 text-sm">
						<Spinner /> {t("Waiting for the project…")}
					</div>
				)}
				{step.kind === "invalid" && (
					<p className="text-sm">{t("This link does not point at a project bundle ConzaCut can open (https only).")}</p>
				)}
				{step.kind === "confirm" && (
					<>
						<p className="text-sm">{t("A link asks to open a project from {host}.", { host: step.host })}</p>
						<p className="text-muted-foreground break-all text-xs">{step.url}</p>
						<p className="text-muted-foreground text-xs">
							{t("It is downloaded into this browser and checked; you choose whether to add it to your projects.")}
						</p>
						<div className="flex gap-2">
							<Button onClick={() => void download(step.url, step.host)} data-testid="open-link-download">
								{t("Download and look")}
							</Button>
							<Button variant="outline" asChild>
								<Link href="/projects">{t("Cancel")}</Link>
							</Button>
						</div>
					</>
				)}
				{step.kind === "downloading" && (
					<div className="flex flex-col gap-2 text-sm">
						<div className="flex items-center gap-2">
							<Spinner />
							{step.total
								? t("Downloading {done} of {total}", { done: formatBytes(step.received), total: formatBytes(step.total) })
								: t("Downloading {done}", { done: formatBytes(step.received) })}
						</div>
						{step.total && (
							<div className="bg-muted h-1.5 overflow-hidden rounded">
								<div className="bg-foreground h-full" style={{ width: `${Math.min(100, (step.received / step.total) * 100)}%` }} />
							</div>
						)}
					</div>
				)}
				{step.kind === "ready" && (
					<>
						<div className="flex flex-col gap-1 text-sm" data-testid="open-link-summary">
							<span className="font-medium">{step.summary.name}</span>
							<span className="text-muted-foreground text-xs">
								{t("{width}×{height} · {seconds} s · {count} media file(s) · {size}", {
									width: step.summary.width,
									height: step.summary.height,
									seconds: Math.round(step.summary.duration),
									count: step.summary.mediaCount,
									size: formatBytes(step.summary.bytes),
								})}
							</span>
							<span className="text-muted-foreground text-xs">{t("From {source}", { source: step.from })}</span>
						</div>
						<div className="flex gap-2">
							<Button onClick={() => void add(step.blob)} data-testid="open-link-add">
								{t("Add to my projects")}
							</Button>
							<Button variant="outline" asChild>
								<Link href="/projects">{t("Cancel")}</Link>
							</Button>
						</div>
					</>
				)}
				{step.kind === "importing" && (
					<div className="text-muted-foreground flex items-center gap-2 text-sm">
						<Spinner /> {t("Adding the project…")}
					</div>
				)}
				{step.kind === "error" && (
					<>
						<p className="text-destructive text-sm" role="alert">
							{step.message}
						</p>
						<Button variant="outline" asChild>
							<Link href="/projects">{t("Go to projects")}</Link>
						</Button>
					</>
				)}
			</div>
		</div>
	);
}

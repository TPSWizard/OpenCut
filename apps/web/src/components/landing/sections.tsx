"use client";

import {
	AppWindow,
	Archive,
	ArrowRight,
	Bot,
	Braces,
	Code,
	Globe,
	Monitor,
	Puzzle,
	SquareTerminal,
	Terminal,
	UserX,
	WifiOff,
	type LucideIcon,
} from "lucide-react";
import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import Link from "@/compat/link";
import { SOCIAL_LINKS } from "@/site/social";
import { useSiteT } from "@/site/use-site-t";
import { Button } from "../ui/button";
import { SectionHeading } from "./section-heading";
import { PrivacyTracks } from "./visuals";

const DESKTOP_GUIDE_URL = `${SOCIAL_LINKS.github}/blob/finish-web-editor/apps/desktop/README.md`;

function Fact({
	icon: Icon,
	title,
	body,
}: {
	icon: LucideIcon;
	title: string;
	body: string;
}) {
	return (
		<div className="landing-reveal flex gap-4">
			<span className="bg-accent flex size-10 shrink-0 items-center justify-center rounded-lg border">
				<Icon aria-hidden="true" className="size-5" />
			</span>
			<div>
				<h3 className="font-semibold tracking-tight">{title}</h3>
				<p className="text-muted-foreground mt-1 text-[0.95rem] leading-relaxed">
					{body}
				</p>
			</div>
		</div>
	);
}

export function Privacy() {
	const t = useSiteT();
	return (
		<section className="border-y px-4 py-20 md:py-28">
			<div className="mx-auto grid w-full max-w-6xl items-center gap-12 lg:grid-cols-2 lg:gap-16">
				<div>
					<SectionHeading
						kicker={t("Private by design")}
						title={t("Your footage never leaves your device.")}
						body={t(
							"ConzaCut edits right in the browser. Media and projects are stored on your device, and captions, cut-outs and reframing run there too. There is no account and nothing to upload.",
						)}
					/>
					<div className="mt-10 grid gap-7 sm:grid-cols-3 lg:grid-cols-1 xl:grid-cols-3">
						<Fact
							icon={UserX}
							title={t("No account")}
							body={t("Open the editor and start cutting.")}
						/>
						<Fact
							icon={WifiOff}
							title={t("Works offline")}
							body={t("Install it and keep editing without a connection.")}
						/>
						<Fact
							icon={Archive}
							title={t("Yours to keep")}
							body={t("Back up a project as one file and open it anywhere.")}
						/>
					</div>
				</div>
				<div className="landing-reveal">
					<PrivacyTracks
						deviceLabel={t("Your device")}
						serverLabel={t("Our servers")}
						emptyLabel={t("empty, by design")}
					/>
				</div>
			</div>
		</section>
	);
}

function PlatformCard({
	icon: Icon,
	title,
	body,
	children,
}: {
	icon: LucideIcon;
	title: string;
	body: string;
	children?: React.ReactNode;
}) {
	return (
		<article className="landing-reveal bg-card flex flex-col rounded-2xl border p-6">
			<Icon aria-hidden="true" className="size-6 text-[var(--trim-ink)]" />
			<h3 className="mt-5 text-lg font-semibold tracking-tight">{title}</h3>
			<p className="text-muted-foreground mt-2 flex-1 text-[0.95rem] leading-relaxed">
				{body}
			</p>
			{children}
		</article>
	);
}

export function Platforms() {
	const t = useSiteT();
	return (
		<section className="px-4 py-20 md:py-28">
			<div className="mx-auto w-full max-w-6xl">
				<SectionHeading
					kicker={t("Works where you are")}
					title={t("In a tab today. On your desktop when you want it.")}
				/>
				<div className="mt-12 grid gap-4 md:grid-cols-3 md:gap-5">
					<PlatformCard
						icon={Globe}
						title={t("In the browser")}
						body={t(
							"Nothing to install. Open a tab on a laptop or desktop and edit with a GPU-composited preview and export.",
						)}
					/>
					<PlatformCard
						icon={AppWindow}
						title={t("As an installed app")}
						body={t(
							"Add it to your dock or taskbar. It opens in its own window and keeps working offline.",
						)}
					/>
					<PlatformCard
						icon={Monitor}
						title={t("Native on the desktop")}
						body={t(
							"A native editor on the same core that opens the same projects. There is no installer yet, so today you build it from source.",
						)}
					>
						<a
							href={DESKTOP_GUIDE_URL}
							target="_blank"
							rel="noopener noreferrer"
							className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium underline-offset-4 hover:underline"
						>
							{t("Read the desktop guide")}
							<ArrowRight aria-hidden="true" className="size-4" />
						</a>
					</PlatformCard>
				</div>
				<p className="text-muted-foreground landing-reveal mt-6 text-sm">
					{t("Editing needs a laptop-size screen for now. Phones are not supported yet.")}
				</p>
			</div>
		</section>
	);
}

const JOB_FILE = `{
  "project": { "name": "Promo", "fps": 30,
    "canvas": { "width": 1080, "height": 1920 } },
  "media": { "intro": "./intro.mp4", "music": "./music.mp3" },
  "timeline": [
    { "type": "clip", "media": "intro", "at": 0 },
    { "type": "text", "text": "Hello", "at": 1, "duration": 3 },
    { "type": "clip", "media": "music", "at": 0,
      "trackType": "audio" }
  ],
  "export": { "format": "mp4", "output": "./promo.mp4" }
}`;

export function Programmable() {
	const t = useSiteT();
	const items: { icon: LucideIcon; title: string; body: string }[] = [
		{
			icon: Braces,
			title: t("Editor API"),
			body: t("One typed JSON surface over projects, timeline, effects and export."),
		},
		{
			icon: Puzzle,
			title: t("Plugins"),
			body: t("Add panels and background workers, sandboxed from the editor."),
		},
		{
			icon: Code,
			title: t("Scripting tab"),
			body: t("Write JavaScript against the open project, inside the editor."),
		},
		{
			icon: SquareTerminal,
			title: t("Headless mode"),
			body: t("Render job files in batches from the command line."),
		},
		{
			icon: Bot,
			title: t("MCP server"),
			body: t("Let AI agents open projects, edit them and render."),
		},
		{
			icon: Terminal,
			title: t("Render CLI"),
			body: t("Turn a project into a video, stills or a contact sheet, natively."),
		},
	];

	return (
		<section className="border-y px-4 py-20 md:py-28">
			<div className="mx-auto grid w-full max-w-6xl gap-12 lg:grid-cols-2 lg:gap-16">
				<div>
					<SectionHeading
						kicker={t("Programmable")}
						title={t("Script it. Automate it. Hand it to an agent.")}
						body={t(
							"Everything the editor does is reachable from code, so the same project can be cut by hand or built by a pipeline.",
						)}
					/>
					<dl className="mt-10 grid gap-x-8 gap-y-7 sm:grid-cols-2">
						{items.map(({ icon: Icon, title, body }) => (
							<div key={title} className="landing-reveal">
								<dt className="flex items-center gap-2.5 font-semibold tracking-tight">
									<Icon
										aria-hidden="true"
										className="size-[1.1rem] text-[var(--trim-ink)]"
									/>
									{title}
								</dt>
								<dd className="text-muted-foreground mt-1.5 text-[0.95rem] leading-relaxed">
									{body}
								</dd>
							</div>
						))}
					</dl>
				</div>

				<figure className="landing-reveal self-center">
					<div className="bg-card overflow-hidden rounded-2xl border">
						<div className="text-muted-foreground flex items-center justify-between border-b px-4 py-2.5 font-mono text-xs">
							<span>job.json</span>
							<span aria-hidden="true">00:00:03:00</span>
						</div>
						<pre
							// The job file is long; it scrolls sideways and must be reachable by keyboard.
							tabIndex={0}
							className="overflow-x-auto p-4 font-mono text-[0.8rem] leading-relaxed sm:p-5"
						>
							<code>{JOB_FILE}</code>
						</pre>
					</div>
					<figcaption className="text-muted-foreground mt-4 text-sm">
						{t("A job file goes in. A finished video comes out.")}
					</figcaption>
				</figure>
			</div>
		</section>
	);
}

export function ClosingCta() {
	const t = useSiteT();
	return (
		<section className="relative isolate overflow-hidden px-4 py-24 text-center md:py-36">
			<div aria-hidden="true" className="landing-grid absolute inset-0 -z-10" />
			<SectionHeading
				align="center"
				kicker={t("Open source")}
				title={t("Make the first cut.")}
				body={t(
					"ConzaCut is free and MIT licensed. Read the code, fork it, or just start editing.",
				)}
			/>
			<div className="landing-reveal mt-9 flex flex-col items-center justify-center gap-4 sm:flex-row">
				<Button asChild size="lg" className="h-11 text-base">
					<Link href="/projects">
						{t("Check it out")}
						<ArrowRight className="ml-0.5" />
					</Link>
				</Button>
				<Button asChild variant="outline" size="lg" className="h-11 text-base">
					<a href={SOCIAL_LINKS.github} target="_blank" rel="noopener noreferrer">
						<HugeiconsIcon icon={GithubIcon} aria-hidden="true" />
						{t("View the source")}
					</a>
				</Button>
			</div>
		</section>
	);
}

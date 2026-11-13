"use client";

import type { ReactNode } from "react";
import { useSiteT } from "@/site/use-site-t";
import { cn } from "@/utils/ui";
import { SectionHeading } from "./section-heading";
import {
	AudioVisual,
	CaptionsVisual,
	ColourVisual,
	ExportVisual,
	KeyframesVisual,
	ReframeVisual,
	TimelineVisual,
	TransitionsVisual,
} from "./visuals";

const PLATFORMS = [
	"YouTube",
	"YouTube Shorts",
	"TikTok",
	"Instagram Reels",
	"LinkedIn",
	"X",
];

function FeatureCard({
	title,
	body,
	visual,
	wide = false,
	className,
	children,
}: {
	title: string;
	body: string;
	visual: ReactNode;
	/** Full-width card: the drawing sits beside the text on wide screens. */
	wide?: boolean;
	className?: string;
	children?: ReactNode;
}) {
	return (
		<article
			className={cn(
				"landing-reveal bg-card flex flex-col overflow-hidden rounded-2xl border",
				wide && "md:flex-row",
				className,
			)}
		>
			<div
				aria-hidden="true"
				className={cn(
					"bg-accent/50 relative h-48 overflow-hidden border-b",
					wide && "md:h-auto md:min-h-52 md:w-1/2 md:border-r md:border-b-0",
				)}
			>
				{visual}
			</div>
			<div
				className={cn(
					"flex flex-1 flex-col p-6",
					wide && "md:justify-center md:p-8",
				)}
			>
				<h3 className="text-lg font-semibold tracking-tight">{title}</h3>
				<p className="text-muted-foreground mt-2 text-[0.95rem] leading-relaxed">
					{body}
				</p>
				{children}
			</div>
		</article>
	);
}

export function Features() {
	const t = useSiteT();

	return (
		<section id="features" className="scroll-mt-20 px-4 pt-10 pb-20 md:pt-12 md:pb-28">
			<div className="mx-auto w-full max-w-6xl">
				<SectionHeading
					kicker={t("What it does")}
					title={t("Everything happens on the timeline.")}
					body={t(
						"Cut, caption, grade and mix in one place. Nothing below is a mock-up: every tool here ships today.",
					)}
				/>

				<div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-6 md:gap-5">
					<FeatureCard
						className="md:col-span-4"
						title={t("A timeline that keeps up")}
						body={t(
							"Multi-track editing with ripple, snapping, groups, compound clips, markers and track locks. Undo anything.",
						)}
						visual={<TimelineVisual />}
					/>
					<FeatureCard
						className="md:col-span-2"
						title={t("Captions that keep time")}
						body={t(
							"Transcribe speech on your device, light up each word as it is spoken, and export SRT, WebVTT or ASS.",
						)}
						visual={
							<CaptionsVisual
								words={t("Every frame stays yours").split(" ")}
							/>
						}
					/>
					<FeatureCard
						className="md:col-span-2"
						title={t("176 transitions")}
						body={t(
							"GPU shader transitions between any two clips, from a clean fade to a cube spin.",
						)}
						visual={<TransitionsVisual />}
					/>
					<FeatureCard
						className="md:col-span-2"
						title={t("Keyframes and curves")}
						body={t(
							"Animate position, scale, colour and volume, then shape the motion with bezier curves.",
						)}
						visual={<KeyframesVisual />}
					/>
					<FeatureCard
						className="md:col-span-2"
						title={t("Colour and effects")}
						body={t(
							"LUTs, curves, levels, lift, gamma and gain, plus grain, glitch, blur and more. Stack them and keyframe them.",
						)}
						visual={<ColourVisual />}
					/>
					<FeatureCard
						className="md:col-span-3"
						title={t("Audio that mixes itself")}
						body={t(
							"Duck music under speech, normalise loudness, reduce noise, cut silence and snap cuts to the beat.",
						)}
						visual={<AudioVisual />}
					/>
					<FeatureCard
						className="md:col-span-3"
						title={t("Smart tools, no cloud")}
						body={t(
							"Reframe to vertical with face tracking, remove backgrounds, find shot changes and drop filler words. All of it runs on your device.",
						)}
						visual={<ReframeVisual />}
					/>
					<FeatureCard
						wide
						className="md:col-span-6"
						title={t("Export for every platform")}
						body={t(
							"Presets set the canvas, frame rate and limits for each platform, with safe-area guides so nothing hides behind the buttons. Export MP4 or WebM.",
						)}
						visual={<ExportVisual />}
					>
						<ul className="mt-5 flex flex-wrap gap-2">
							{PLATFORMS.map((platform) => (
								<li
									key={platform}
									className="text-muted-foreground rounded-md border px-2.5 py-1 text-sm"
								>
									{platform}
								</li>
							))}
						</ul>
					</FeatureCard>
				</div>
			</div>
		</section>
	);
}

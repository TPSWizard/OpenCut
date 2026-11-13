import type { CSSProperties, ReactNode } from "react";
import { cn } from "@/utils/ui";

/*
 * Small drawings for the feature cards, built from the editor's own shapes.
 * They are decoration: every one is rendered inside an `aria-hidden` frame,
 * and all sizes come from integer arithmetic so the server and the browser
 * always produce the same markup.
 */

/** The four generated clips from the product shot, as CSS gradients. */
export const CLIP_GRADIENTS = {
	sunrise: "linear-gradient(135deg, #f5b400 5%, #ff5a36 62%, #1b2a6b)",
	tide: "linear-gradient(120deg, #7ce8ff, #0b5ed7 45%, #16c2d5 78%, #041c3a)",
	dusk: "linear-gradient(135deg, #ff3d81, #ffb86b 55%, #2d1b69)",
	meadow: "linear-gradient(120deg, #38d67a, #d8f34a 62%, #0c3b2a)",
} as const;

type ClipName = keyof typeof CLIP_GRADIENTS;

/** Bar heights (15 to 100) for a waveform; the same seed always gives the same shape. */
export function waveform({
	count,
	seed,
}: {
	count: number;
	seed: number;
}): number[] {
	return Array.from({ length: count }, (_, index) => {
		const swell = Math.abs(((index * 7 + seed * 3) % 40) - 20);
		const grit = (index * index * 31 + index * 17 + seed * 13) % 23;
		return Math.min(100, 15 + swell * 3 + grit);
	});
}

function Bars({
	heights,
	className,
	barClassName,
}: {
	heights: number[];
	className?: string;
	barClassName?: string;
}) {
	return (
		<div className={cn("flex h-full items-end gap-px", className)}>
			{heights.map((height, index) => (
				<span
					key={index}
					className={cn("min-w-0 flex-1 rounded-[1px]", barClassName)}
					style={{ height: `${height}%` }}
				/>
			))}
		</div>
	);
}

function VideoClip({
	clip,
	className,
	style,
	children,
}: {
	clip: ClipName;
	className?: string;
	style?: CSSProperties;
	children?: ReactNode;
}) {
	return (
		<div
			className={cn("relative h-full rounded-[5px]", className)}
			style={{ backgroundImage: CLIP_GRADIENTS[clip], ...style }}
		>
			{children}
		</div>
	);
}

function Playhead({ className }: { className?: string }) {
	return (
		<span
			className={cn(
				"landing-playhead absolute inset-y-2 left-[38%] w-px bg-[var(--playhead)]",
				className,
			)}
		>
			<span className="absolute -top-1 -left-[5px] size-[11px] rounded-full bg-[var(--playhead)]" />
		</span>
	);
}

const SCORE = waveform({ count: 72, seed: 4 });
const VOICE = waveform({ count: 72, seed: 11 });

/** A small multi-track timeline with a moving playhead. */
export function TimelineVisual() {
	return (
		<div className="absolute inset-0 flex flex-col justify-center gap-1.5 px-5">
			<div className="landing-ticks mb-1 h-3 opacity-70" />
			<div className="flex h-5 gap-1 pl-[9%]">
				<div className="w-[30%] rounded-[5px] bg-[var(--clip-text)]" />
				<div className="ml-[14%] w-[18%] rounded-[5px] bg-[var(--clip-text)]" />
				<div className="w-[20%] rounded-[5px] bg-[var(--clip-text)]" />
			</div>
			<div className="flex h-10 gap-1">
				<VideoClip clip="sunrise" className="w-[27%]" />
				<VideoClip clip="tide" className="w-[23%] ring-2 ring-[var(--trim)] ring-inset" />
				<VideoClip clip="dusk" className="w-[28%]" />
				<VideoClip clip="meadow" className="flex-1" />
			</div>
			<div className="h-7 overflow-hidden rounded-[5px] bg-[var(--clip-audio)] px-1 pt-2">
				<Bars heights={SCORE} barClassName="bg-white/60" />
			</div>
			<Playhead />
		</div>
	);
}

/** A caption whose words light up one after another. */
export function CaptionsVisual({ words }: { words: string[] }) {
	return (
		<div className="absolute inset-0 flex flex-col items-center justify-center gap-5 px-5">
			<div
				className="flex w-full max-w-xs flex-col items-center justify-end rounded-lg pt-14 pb-3"
				style={{ backgroundImage: CLIP_GRADIENTS.tide }}
			>
				<p className="flex flex-wrap justify-center gap-x-1 text-base font-bold text-white [text-shadow:0_1px_2px_rgb(0_0_0/0.45)]">
					{words.map((word, index) => (
						<span
							key={index}
							className={cn(
								"landing-word rounded px-1 [text-shadow:none]",
								index === 1 && "landing-word-static",
							)}
							style={{ "--i": index } as CSSProperties}
						>
							{word}
						</span>
					))}
				</p>
			</div>
			<div className="text-muted-foreground flex gap-2 font-mono text-[0.68rem]">
				{["SRT", "VTT", "ASS"].map((format) => (
					<span key={format} className="rounded border px-1.5 py-0.5">
						.{format.toLowerCase()}
					</span>
				))}
			</div>
		</div>
	);
}

/** One clip wiping into the next along a diagonal. */
export function TransitionsVisual() {
	return (
		<div className="absolute inset-0 flex items-center justify-center px-5">
			<div className="relative h-24 w-full max-w-xs overflow-hidden rounded-lg">
				<div
					className="absolute inset-0"
					style={{ backgroundImage: CLIP_GRADIENTS.dusk }}
				/>
				<div
					className="landing-wipe absolute inset-0 [clip-path:polygon(0_0,62%_0,50%_100%,0_100%)]"
					style={{ backgroundImage: CLIP_GRADIENTS.sunrise }}
				/>
			</div>
		</div>
	);
}

/** A bezier curve through three keyframes. */
export function KeyframesVisual() {
	const keys = [
		{ x: 16, y: 78 },
		{ x: 150, y: 26 },
		{ x: 284, y: 58 },
	];
	return (
		<div className="absolute inset-0 flex items-center justify-center px-5">
			<svg
				viewBox="0 0 300 100"
				className="text-foreground h-28 w-full max-w-xs overflow-visible"
				fill="none"
				role="presentation"
			>
				<path
					d="M0 78 H300 M0 26 H300"
					stroke="currentColor"
					strokeOpacity="0.1"
					strokeDasharray="3 5"
				/>
				<path
					d="M16 78 C70 78 96 26 150 26 C204 26 230 58 284 58"
					stroke="var(--playhead)"
					strokeWidth="2"
				/>
				<path
					d="M150 26 H206 M150 26 H94"
					stroke="currentColor"
					strokeOpacity="0.35"
				/>
				<circle cx="206" cy="26" r="3" fill="currentColor" fillOpacity="0.6" />
				<circle cx="94" cy="26" r="3" fill="currentColor" fillOpacity="0.6" />
				{keys.map((key) => (
					<rect
						key={key.x}
						x={key.x - 6}
						y={key.y - 6}
						width="12"
						height="12"
						rx="2"
						transform={`rotate(45 ${key.x} ${key.y})`}
						fill="var(--trim)"
						stroke="var(--background)"
						strokeWidth="2"
					/>
				))}
			</svg>
		</div>
	);
}

/** The same frame before and after a grade, split by a trim handle. */
export function ColourVisual() {
	return (
		<div className="absolute inset-0 flex items-center justify-center px-5">
			<div className="relative h-24 w-full max-w-xs overflow-hidden rounded-lg">
				<div
					className="absolute inset-0"
					style={{ backgroundImage: CLIP_GRADIENTS.meadow }}
				/>
				<div
					className="absolute inset-y-0 left-0 w-1/2 [filter:saturate(0.25)_contrast(0.85)_brightness(0.95)]"
					style={{
						backgroundImage: CLIP_GRADIENTS.meadow,
						backgroundSize: "200% 100%",
					}}
				/>
				<span className="absolute inset-y-0 left-1/2 w-px bg-[var(--trim)]">
					<span className="bg-background absolute top-1/2 left-1/2 flex h-9 w-4 -translate-1/2 items-center justify-center rounded-full border border-[var(--trim)]">
						<span className="h-4 w-1 rounded-full bg-[var(--trim)]" />
					</span>
				</span>
			</div>
		</div>
	);
}

/** Music that dips while the voice is speaking. */
export function AudioVisual() {
	const speaking = (index: number) =>
		(index >= 10 && index < 30) || (index >= 44 && index < 62);
	const voice = VOICE.map((height, index) => (speaking(index) ? height : 6));
	const music = SCORE.map((height, index) =>
		speaking(index) ? Math.round(height * 0.3) : height,
	);
	return (
		<div className="absolute inset-0 flex flex-col justify-center gap-2 px-5">
			<div className="h-9 rounded-[5px] bg-[var(--clip-text)]/25 px-1 py-1">
				<Bars heights={voice} barClassName="bg-[var(--clip-text)]" />
			</div>
			<div className="relative h-12 rounded-[5px] bg-[var(--clip-audio)]/25 px-1 py-1">
				<Bars heights={music} barClassName="bg-[var(--clip-audio)]" />
				<svg
					viewBox="0 0 72 20"
					preserveAspectRatio="none"
					className="absolute inset-0 size-full"
					fill="none"
					role="presentation"
				>
					<path
						d="M0 3 H8 L11 14 H29 L32 3 H42 L45 14 H61 L64 3 H72"
						stroke="var(--trim)"
						strokeWidth="1.5"
						vectorEffect="non-scaling-stroke"
					/>
				</svg>
			</div>
		</div>
	);
}

/** A vertical crop window that follows the subject across a wide frame. */
export function ReframeVisual() {
	return (
		<div className="absolute inset-0 flex items-center justify-center px-5">
			<div
				className="relative h-28 w-full max-w-sm overflow-hidden rounded-lg"
				style={{ backgroundImage: CLIP_GRADIENTS.dusk }}
			>
				<div className="absolute inset-0 bg-black/45" />
				<div className="landing-follow absolute inset-y-0 left-1/2 aspect-[9/16] -translate-x-1/2">
					<div
						className="absolute inset-0 rounded-md ring-2 ring-[var(--trim)]"
						style={{
							backgroundImage: CLIP_GRADIENTS.dusk,
							backgroundSize: "600% 100%",
							backgroundPosition: "center",
						}}
					/>
					<span className="absolute top-[34%] left-1/2 size-5 -translate-x-1/2 rounded-full bg-white/90" />
					<span className="absolute top-[56%] left-1/2 h-9 w-8 -translate-x-1/2 rounded-t-full bg-white/90" />
				</div>
			</div>
		</div>
	);
}

/** The export canvases, each with its safe area marked. */
export function ExportVisual() {
	const frames = [
		{ label: "16:9", className: "aspect-video h-16", clip: "sunrise" },
		{ label: "9:16", className: "aspect-[9/16] h-28", clip: "tide" },
		{ label: "16:9", className: "aspect-video h-12", clip: "meadow" },
	] as const;
	return (
		<div className="absolute inset-0 flex items-center justify-center gap-5 px-5">
			{frames.map((frame, index) => (
				<div
					key={index}
					className={cn("relative shrink-0 rounded-md", frame.className)}
					style={{ backgroundImage: CLIP_GRADIENTS[frame.clip] }}
				>
					<span
						className={cn(
							"absolute rounded-sm border border-dashed border-white/80",
							frame.label === "9:16"
								? "inset-x-[8%] top-[10%] bottom-[20%]"
								: "inset-[8%]",
						)}
					/>
					<span className="absolute right-1 bottom-1 rounded-sm bg-black/55 px-1 font-mono text-[0.6rem] text-white">
						{frame.label}
					</span>
				</div>
			))}
		</div>
	);
}

/** Two tracks: everything on the device, nothing on a server. */
export function PrivacyTracks({
	deviceLabel,
	serverLabel,
	emptyLabel,
}: {
	deviceLabel: string;
	serverLabel: string;
	emptyLabel: string;
}) {
	return (
		<div className="bg-card relative overflow-hidden rounded-2xl border p-5 text-left sm:p-7">
			<div aria-hidden="true" className="landing-ticks mb-5 h-3 opacity-60" />
			<div className="grid grid-cols-[auto_1fr] items-center gap-x-4 gap-y-3 sm:gap-x-6">
				<p className="text-sm font-medium">{deviceLabel}</p>
				<div aria-hidden="true" className="flex flex-col gap-1.5">
					<div className="flex h-9 gap-1">
						<VideoClip clip="sunrise" className="w-[30%]" />
						<VideoClip clip="tide" className="w-[22%]" />
						<VideoClip clip="dusk" className="w-[26%]" />
						<VideoClip clip="meadow" className="flex-1" />
					</div>
					<div className="h-6 overflow-hidden rounded-[5px] bg-[var(--clip-audio)] px-1 pt-1.5">
						<Bars heights={SCORE} barClassName="bg-white/60" />
					</div>
				</div>
				<p className="text-muted-foreground text-sm font-medium">
					{serverLabel}
				</p>
				<div className="text-muted-foreground flex h-12 items-center justify-center rounded-[5px] border border-dashed font-mono text-xs">
					{emptyLabel}
				</div>
			</div>
		</div>
	);
}

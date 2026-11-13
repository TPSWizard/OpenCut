"use client";

import { useSiteT } from "@/site/use-site-t";
import { cn } from "@/utils/ui";

const SHOT_WIDTH = 2400;
const SHOT_HEIGHT = 1410;
const SIZES = "(min-width: 1216px) 1152px, calc(100vw - 2rem)";

/**
 * One theme's screenshot. Both themes are in the page and CSS shows the one
 * that matches; a hidden lazy image is never fetched.
 */
function Shot({
	theme,
	alt,
	className,
}: {
	theme: "dark" | "light";
	alt: string;
	className: string;
}) {
	const base = `/landing/editor-${theme}`;
	return (
		<picture className={className}>
			<source
				type="image/avif"
				srcSet={`${base}-1280.avif 1280w, ${base}.avif ${SHOT_WIDTH}w`}
				sizes={SIZES}
			/>
			<source
				type="image/webp"
				srcSet={`${base}-1280.webp 1280w, ${base}.webp ${SHOT_WIDTH}w`}
				sizes={SIZES}
			/>
			<img
				src={`${base}-1280.webp`}
				width={SHOT_WIDTH}
				height={SHOT_HEIGHT}
				alt={alt}
				loading="lazy"
				decoding="async"
				className="size-full object-cover object-[50%_60%] sm:h-auto sm:object-fill"
			/>
		</picture>
	);
}

/**
 * The real editor, captured from the running app with a project whose clips
 * and audio were generated for this page (see `ATTRIBUTION.md`).
 */
export function ProductShot({ className }: { className?: string }) {
	const t = useSiteT();
	const alt = t(
		"The ConzaCut editor: a media bin, a preview with a highlighted caption, an inspector, and a timeline with caption, video and audio tracks.",
	);

	return (
		<figure
			className={cn("relative mx-auto mt-14 w-full max-w-6xl md:mt-20", className)}
		>
			<div
				aria-hidden="true"
				className="landing-glow absolute inset-x-0 -top-16 -z-10 h-56"
			/>
			<div className="bg-card aspect-[4/3] overflow-hidden rounded-xl border shadow-2xl shadow-black/40 sm:aspect-[2400/1410] md:rounded-2xl">
				<Shot theme="dark" alt={alt} className="hidden size-full dark:block" />
				<Shot theme="light" alt={alt} className="block size-full dark:hidden" />
			</div>
			<figcaption className="text-muted-foreground mt-4 text-sm">
				{t(
					"The real editor, mid-project. Every clip on this timeline was generated for this page.",
				)}
			</figcaption>
		</figure>
	);
}

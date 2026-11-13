/**
 * The page as a timeline: a ruler pinned to the bottom of the window whose
 * playhead follows the scroll position, with a running timecode. It is pure
 * CSS (see `landing.css`), decorative, and only appears on wide screens in
 * browsers with scroll-driven animations and for visitors who have not asked
 * for reduced motion.
 */
export function Scrubber() {
	return (
		<div
			aria-hidden="true"
			className="landing-scrubber bg-background/85 pointer-events-none fixed inset-x-0 bottom-0 z-20 h-7 border-t backdrop-blur"
		>
			<div className="landing-ticks absolute inset-x-0 bottom-0 h-3 opacity-60" />
			<span className="landing-timecode text-muted-foreground absolute top-1 left-3 font-mono text-[0.68rem] tabular-nums" />
			<span className="landing-scrubber-head absolute inset-y-0 w-px bg-[var(--playhead)]">
				<span className="absolute -top-px -left-[5px] h-2.5 w-[11px] rounded-b-[5px] bg-[var(--playhead)]" />
			</span>
		</div>
	);
}

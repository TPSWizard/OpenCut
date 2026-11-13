import { cn } from "@/utils/ui";

/** The heading every landing section opens with: a kicker, a title, a line of body. */
export function SectionHeading({
	kicker,
	title,
	body,
	align = "left",
	className,
}: {
	kicker: string;
	title: string;
	body?: string;
	align?: "left" | "center";
	className?: string;
}) {
	return (
		<div
			className={cn(
				"landing-reveal max-w-2xl",
				align === "center" && "mx-auto text-center",
				className,
			)}
		>
			<p
				className={cn(
					"flex items-center gap-2.5 font-mono text-xs tracking-wide text-[var(--trim-ink)] uppercase",
					align === "center" && "justify-center",
				)}
			>
				<span
					aria-hidden="true"
					className="inline-block h-3 w-1 rounded-full bg-[var(--trim)]"
				/>
				{kicker}
			</p>
			<h2 className="mt-4 text-3xl font-bold tracking-tighter text-balance md:text-5xl">
				{title}
			</h2>
			{body ? (
				<p className="text-muted-foreground mt-4 text-base leading-relaxed text-pretty sm:text-lg">
					{body}
				</p>
			) : null}
		</div>
	);
}

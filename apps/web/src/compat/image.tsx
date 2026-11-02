import type { ComponentProps } from "react";

/**
 * Drop-in stand-in for `next/image` used by the editor code that was ported
 * from the Next.js app. Vite serves static assets directly, so this is a plain
 * `<img>` that accepts (and ignores) the Next-only props.
 */
export type ImageProps = Omit<ComponentProps<"img">, "src" | "alt"> & {
	src: string;
	alt: string;
	fill?: boolean;
	priority?: boolean;
	unoptimized?: boolean;
	quality?: number;
	sizes?: string;
};

export default function Image({
	fill,
	priority,
	unoptimized: _unoptimized,
	quality: _quality,
	style,
	...props
}: ImageProps) {
	const fillStyle = fill
		? ({
				position: "absolute",
				inset: 0,
				width: "100%",
				height: "100%",
				objectFit: style?.objectFit ?? "cover",
			} as const)
		: undefined;

	return (
		<img
			loading={priority ? "eager" : props.loading}
			decoding="async"
			{...props}
			style={fillStyle ? { ...fillStyle, ...style } : style}
		/>
	);
}

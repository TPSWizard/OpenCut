import { Link as RouterLink } from "@tanstack/react-router";
import type { ComponentProps, ReactNode } from "react";

/**
 * Stand-in for `next/link`. Internal paths go through TanStack Router so
 * navigation stays client-side; absolute URLs render a plain anchor.
 */
export type LinkProps = Omit<ComponentProps<"a">, "href"> & {
	href: string;
	children?: ReactNode;
	prefetch?: boolean;
	scroll?: boolean;
	replace?: boolean;
};

function isExternal(href: string) {
	return /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(href) || href.startsWith("#");
}

export default function Link({
	href,
	prefetch: _prefetch,
	scroll: _scroll,
	replace,
	children,
	...props
}: LinkProps) {
	if (isExternal(href)) {
		return (
			<a href={href} {...props}>
				{children}
			</a>
		);
	}

	return (
		// The ported code builds paths dynamically, so the route is not statically typed here.
		<RouterLink to={href as never} replace={replace} {...(props as object)}>
			{children}
		</RouterLink>
	);
}

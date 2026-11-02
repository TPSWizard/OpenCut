import {
	useLocation,
	useNavigate,
	useParams as useRouterParams,
	useRouter as useTanStackRouter,
} from "@tanstack/react-router";
import { useMemo } from "react";

/**
 * Stand-in for the `next/navigation` hooks the ported editor code relies on,
 * backed by TanStack Router.
 */
export function useRouter() {
	const navigate = useNavigate();
	const router = useTanStackRouter();

	return useMemo(
		() => ({
			push: (href: string) => {
				void navigate({ to: href as never });
			},
			replace: (href: string) => {
				void navigate({ to: href as never, replace: true });
			},
			back: () => {
				router.history.back();
			},
			forward: () => {
				router.history.forward();
			},
			refresh: () => {
				void router.invalidate();
			},
			prefetch: (_href: string) => {},
		}),
		[navigate, router],
	);
}

export function usePathname(): string {
	const location = useLocation();
	return location.pathname;
}

export function useParams(): Record<string, string> {
	return useRouterParams({ strict: false }) as Record<string, string>;
}

export function useSearchParams(): URLSearchParams {
	const location = useLocation();
	const search = location.searchStr;
	return useMemo(() => new URLSearchParams(search), [search]);
}

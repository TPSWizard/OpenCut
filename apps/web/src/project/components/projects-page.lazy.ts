import { lazyRouteComponent } from "@tanstack/react-router";

const load = () => import("./projects-page");

/**
 * Client-only entry for the projects page. The module is loaded lazily and
 * left out of the server bundle (`import.meta.env.SSR` is replaced at build
 * time). It lives outside the route file so TanStack's route code-splitter
 * never has to rewrite the dynamic import.
 */
export const ProjectsPageLazy = lazyRouteComponent(
	() =>
		import.meta.env.SSR
			? (Promise.resolve({ ProjectsPage: () => null }) as unknown as ReturnType<
					typeof load
				>)
			: load(),
	"ProjectsPage",
);

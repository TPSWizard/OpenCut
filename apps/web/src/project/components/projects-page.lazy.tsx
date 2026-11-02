import { Suspense, lazy } from "react";
import { PageLoading } from "@/components/page-loading";

const ProjectsPage = lazy(() =>
	import("./projects-page").then((module) => ({ default: module.ProjectsPage })),
);

/**
 * Client-only entry for the projects page. The page module is loaded lazily
 * and left out of the server bundle (`import.meta.env.SSR` is replaced at
 * build time), so the server never evaluates the editor core.
 */
export function ProjectsPageLazy() {
	if (import.meta.env.SSR) return <PageLoading />;
	return (
		<Suspense fallback={<PageLoading />}>
			<ProjectsPage />
		</Suspense>
	);
}

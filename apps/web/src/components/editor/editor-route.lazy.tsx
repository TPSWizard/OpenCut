import { Suspense, lazy } from "react";
import { PageLoading } from "@/components/page-loading";

const EditorRoute = lazy(() =>
	import("./editor-route").then((module) => ({ default: module.EditorRoute })),
);

/**
 * Client-only entry for the editor. The editor needs WebCodecs, OPFS and
 * WebGPU, so it is loaded lazily and left out of the server bundle
 * (`import.meta.env.SSR` is replaced at build time).
 */
export function EditorRouteLazy() {
	if (import.meta.env.SSR) return <PageLoading />;
	return (
		<Suspense fallback={<PageLoading />}>
			<EditorRoute />
		</Suspense>
	);
}

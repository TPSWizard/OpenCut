import { Suspense, lazy } from "react";
import { PageLoading } from "@/components/page-loading";

const OpenLinkPage = lazy(() => import("./open-link-page").then((module) => ({ default: module.OpenLinkPage })));

/** Client-only entry for `/open` (projects live in the browser). */
export function OpenLinkPageLazy() {
	if (import.meta.env.SSR) return <PageLoading />;
	return (
		<Suspense fallback={<PageLoading />}>
			<OpenLinkPage />
		</Suspense>
	);
}

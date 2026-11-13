import { createFileRoute } from "@tanstack/react-router";
import { OpenLinkPageLazy } from "@/project/components/open-link-page.lazy";

// "Open in ConzaCut": /open?bundle=<url> or /open?handoff=1 (see OpenLinkPage).
// Client only, like the projects page.
export const Route = createFileRoute("/open")({
	ssr: false,
	component: OpenLinkPageLazy,
});

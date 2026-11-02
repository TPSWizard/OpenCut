import { createFileRoute } from "@tanstack/react-router";
import { ProjectsPageLazy } from "@/project/components/projects-page.lazy";

// Projects live in the browser (IndexedDB / OPFS), so this route only renders
// on the client.
export const Route = createFileRoute("/projects")({
	ssr: false,
	component: ProjectsPageLazy,
});

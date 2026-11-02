import { createFileRoute, redirect } from "@tanstack/react-router";

// `/editor` on its own has no project to show; the projects page is where a
// project is picked or created. (`/editor/$projectId` is a sibling route and
// is not affected by this redirect.)
export const Route = createFileRoute("/editor/")({
	beforeLoad: () => {
		throw redirect({ to: "/projects" });
	},
});

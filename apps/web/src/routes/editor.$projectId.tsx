import { createFileRoute } from "@tanstack/react-router";
import { EditorRouteLazy } from "@/components/editor/editor-route.lazy";

// The editor runs entirely in the browser (WebCodecs, OPFS, WebGPU), so the
// route is client-only.
export const Route = createFileRoute("/editor/$projectId")({
	ssr: false,
	component: EditorRouteLazy,
});

import { useParams } from "@tanstack/react-router";
import { EditorPage } from "./editor-page";

export function EditorRoute() {
	const { projectId } = useParams({ from: "/editor/$projectId" });
	return <EditorPage projectId={projectId} />;
}

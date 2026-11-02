import { lazyRouteComponent } from "@tanstack/react-router";

const load = () => import("./editor-route");

/**
 * Client-only entry for the editor. The editor needs WebCodecs, OPFS and
 * WebGPU, so it is loaded lazily and left out of the server bundle
 * (`import.meta.env.SSR` is replaced at build time).
 */
export const EditorRouteLazy = lazyRouteComponent(
	() =>
		import.meta.env.SSR
			? (Promise.resolve({ EditorRoute: () => null }) as unknown as ReturnType<
					typeof load
				>)
			: load(),
	"EditorRoute",
);

/**
 * Base URL of the OpenCut API worker (`apps/api`). Defaults to the local
 * `wrangler dev` port so `moon run web:dev` and `moon run api:dev` work
 * together without configuration.
 */
const DEFAULT_API_URL = "http://localhost:8787";

export function getApiBaseUrl(): string {
	const configured = import.meta.env.VITE_API_URL as string | undefined;
	return (configured && configured.length > 0 ? configured : DEFAULT_API_URL).replace(/\/+$/, "");
}

export function apiUrl(path: string): string {
	return `${getApiBaseUrl()}${path.startsWith("/") ? path : `/${path}`}`;
}

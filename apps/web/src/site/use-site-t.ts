import { useEffect, useState } from "react";
import { translate, useT } from "@/i18n";

export type SiteTranslate = (
	text: string,
	params?: Record<string, string | number>,
) => string;

const english: SiteTranslate = (text, params) =>
	translate({ text, locale: "en", params });

/** False on the server and for the first client render, true once the page has hydrated. */
export function useHydrated(): boolean {
	const [hydrated, setHydrated] = useState(false);
	useEffect(() => setHydrated(true), []);
	return hydrated;
}

/**
 * `t` for pages that are rendered on the server (the landing page, the
 * header and the footer). The server cannot know the visitor's language, so
 * the first client render repeats the server's English and the active
 * language takes over once the page has hydrated.
 */
export function useSiteT(): SiteTranslate {
	const t = useT();
	return useHydrated() ? t : english;
}

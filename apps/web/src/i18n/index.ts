/**
 * Interface translation. English text is the key: `t('Export')` shows the
 * catalogue's entry for the active language, or the English when there is
 * none, so a string that is not translated yet still reads correctly.
 * `{name}` placeholders are filled from `params`.
 *
 * Catalogues are JSON files under `locales/<locale>/` (one per area, merged
 * at build time), each a map from English text to the translation. The
 * desktop editor keeps its own catalogue in the same shape.
 */

import { useSyncExternalStore } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Locale = "en" | "zh-CN";
export type LocalePreference = Locale | "auto";

export const LOCALES: { id: Locale; label: string }[] = [
	{ id: "en", label: "English" },
	{ id: "zh-CN", label: "简体中文" },
];

type Catalog = Record<string, string>;

function mergeCatalogs(modules: Record<string, unknown>): Catalog {
	const merged: Catalog = {};
	for (const module of Object.values(modules)) {
		const entries = ((module as { default?: unknown }).default ?? module) as Record<string, unknown>;
		for (const [key, value] of Object.entries(entries)) {
			if (typeof value === "string" && value.length > 0) merged[key] = value;
		}
	}
	return merged;
}

const CATALOGS: Record<Locale, Catalog> = {
	en: {},
	"zh-CN": mergeCatalogs(import.meta.glob("./locales/zh-CN/*.json", { eager: true })),
};

export function catalogFor(locale: Locale): Catalog {
	return CATALOGS[locale];
}

/** The language to show: the chosen one, or the browser's when it is one we have. */
export function resolveLocale({
	preference,
	languages,
}: {
	preference: LocalePreference;
	languages: readonly string[];
}): Locale {
	if (preference !== "auto") return preference;
	for (const language of languages) {
		const lower = language.toLowerCase();
		if (lower.startsWith("zh")) return "zh-CN";
		if (lower.startsWith("en")) return "en";
	}
	return "en";
}

interface LocaleState {
	preference: LocalePreference;
	setPreference: (preference: LocalePreference) => void;
}

export const useLocaleStore = create<LocaleState>()(
	persist(
		(set) => ({
			preference: "auto",
			setPreference: (preference) => set({ preference }),
		}),
		{ name: "interface-language", version: 1 },
	),
);

function browserLanguages(): readonly string[] {
	if (typeof navigator === "undefined") return [];
	return navigator.languages?.length ? navigator.languages : [navigator.language].filter(Boolean);
}

export function currentLocale(): Locale {
	return resolveLocale({ preference: useLocaleStore.getState().preference, languages: browserLanguages() });
}

function fill(text: string, params?: Record<string, string | number>): string {
	if (!params) return text;
	return text.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match));
}

/** The text in a given language (English when it has no entry). */
export function translate({
	text,
	locale,
	params,
}: {
	text: string;
	locale: Locale;
	params?: Record<string, string | number>;
}): string {
	return fill(CATALOGS[locale][text] ?? text, params);
}

/** The text in the active language. Outside React, or for text read once. */
export function t(text: string, params?: Record<string, string | number>): string {
	return translate({ text, locale: currentLocale(), params });
}

/** `t` for components: re-renders when the language changes. */
export function useT(): typeof t {
	useSyncExternalStore(
		useLocaleStore.subscribe,
		() => useLocaleStore.getState().preference,
		() => "auto" as LocalePreference,
	);
	return t;
}

/** Keeps `<html lang>` in step with the active language (for fonts and screen readers). */
export function syncDocumentLanguage(): () => void {
	const apply = () => {
		if (typeof document !== "undefined") document.documentElement.lang = currentLocale();
	};
	apply();
	return useLocaleStore.subscribe(apply);
}

"use client";

import { useEffect } from "react";
import { Select, SelectContent, SelectItem, SelectSeparator, SelectTrigger } from "@/components/ui/select";
import { LOCALES, currentLocale, syncDocumentLanguage, useLocaleStore, useT, type LocalePreference } from "@/i18n";

/** Picks the interface language (Automatic follows the browser). */
export function LanguageSelect({ className }: { className?: string }) {
	const t = useT();
	const preference = useLocaleStore((state) => state.preference);
	const setPreference = useLocaleStore((state) => state.setPreference);
	const active = LOCALES.find((locale) => locale.id === currentLocale())?.label ?? "English";
	return (
		<Select value={preference} onValueChange={(value) => setPreference(value as LocalePreference)}>
			<SelectTrigger className={className} aria-label={t("Interface language")} title={t("Interface language")} data-testid="language-select">
				{preference === "auto" ? `${t("Automatic")} (${active})` : active}
			</SelectTrigger>
			<SelectContent>
				<SelectItem value="auto">{t("Automatic")}</SelectItem>
				<SelectSeparator />
				{LOCALES.map((locale) => (
					<SelectItem key={locale.id} value={locale.id}>
						{locale.label}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

/** Keeps `<html lang>` matching the interface language. */
export function LanguageRoot() {
	useEffect(() => syncDocumentLanguage(), []);
	return null;
}

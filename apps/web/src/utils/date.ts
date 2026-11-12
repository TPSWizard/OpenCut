import { currentLocale } from "@/i18n";
export function formatDate({ date }: { date: Date }): string {
	// Dates follow the interface language (2026年11月12日 in Chinese).
	return date.toLocaleDateString(currentLocale() === "zh-CN" ? "zh-CN" : "en-US", {
		month: "short",
		day: "numeric",
		year: "numeric",
	});
}

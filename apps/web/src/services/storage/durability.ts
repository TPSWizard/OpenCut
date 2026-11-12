/**
 * How safe this browser's copy of the user's projects is. Browsers may
 * clear site storage when the disk runs low unless it is "persisted";
 * Safari also deletes everything a site stored after seven days without a
 * visit unless the site runs as an installed app. These rules turn what
 * the browser reports into a level, reasons and advice for the user.
 */

export type BrowserFamily = "safari" | "firefox" | "chromium" | "other";

export type DurabilityLevel = "protected" | "at-risk" | "unknown";

export interface DurabilityInput {
	/** `navigator.storage.persisted()`, or null where the API is missing. */
	persisted: boolean | null;
	browser: BrowserFamily;
	/** Running as an installed app (display-mode standalone). */
	standalone: boolean;
	/** Milliseconds since the previous visit, when known. */
	sinceLastVisitMs?: number | null;
}

export interface DurabilityAssessment {
	level: DurabilityLevel;
	reasons: string[];
	advice: string[];
	/** Safari's seven-day rule applies to this tab. */
	sevenDayRule: boolean;
}

export const SAFARI_EVICTION_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

export function detectBrowserFamily({ userAgent, vendor = "" }: { userAgent: string; vendor?: string }): BrowserFamily {
	const ua = userAgent.toLowerCase();
	if (ua.includes("firefox") || ua.includes("fxios")) return "firefox";
	// Chrome, Edge, Opera and other Chromium browsers also say "Safari".
	if (ua.includes("chrome") || ua.includes("chromium") || ua.includes("crios") || ua.includes("edg")) return "chromium";
	if (ua.includes("safari") && (vendor.toLowerCase().includes("apple") || ua.includes("version/"))) return "safari";
	return "other";
}

export function assessDurability({ persisted, browser, standalone, sinceLastVisitMs }: DurabilityInput): DurabilityAssessment {
	const reasons: string[] = [];
	const advice: string[] = [];
	const sevenDayRule = browser === "safari" && !standalone;

	if (sevenDayRule) {
		reasons.push(
			`Safari deletes everything this site stores after ${SAFARI_EVICTION_DAYS} days without a visit, projects and media included.`,
		);
		advice.push("Add ConzaCut to the Dock or Home Screen (Share, then Add to Dock / Add to Home Screen); installed apps keep their data.");
		advice.push("Back up projects you care about as .opencut.zip bundles.");
		if (typeof sinceLastVisitMs === "number" && sinceLastVisitMs >= (SAFARI_EVICTION_DAYS - 2) * DAY_MS) {
			reasons.push(`Your last visit was ${Math.floor(sinceLastVisitMs / DAY_MS)} days ago.`);
		}
	}

	if (persisted === false) {
		reasons.push("The browser may clear this site's storage when the disk runs low.");
		advice.unshift("Allow persistent storage so the browser keeps your projects.");
	}

	if (persisted === null) {
		reasons.push("This browser does not say whether it keeps this site's storage.");
		advice.push("Back up projects you care about as .opencut.zip bundles.");
	}

	const level: DurabilityLevel = sevenDayRule || persisted === false ? "at-risk" : persisted === null ? "unknown" : "protected";
	return { level, reasons, advice: [...new Set(advice)], sevenDayRule };
}

/** Whether a project has changed since its last backup (or never had one). */
export function needsBackup({ updatedAt, lastBackupAt }: { updatedAt: number; lastBackupAt?: number | null }): boolean {
	return !lastBackupAt || updatedAt > lastBackupAt;
}

/** "never", "today", "yesterday" or "N days ago". */
export function describeBackupAge({ lastBackupAt, now }: { lastBackupAt?: number | null; now: number }): string {
	if (!lastBackupAt) return "never";
	const days = Math.floor((now - lastBackupAt) / DAY_MS);
	if (days <= 0) return "today";
	if (days === 1) return "yesterday";
	return `${days} days ago`;
}

// ---------------------------------------------------------------- browser state

const LAST_VISIT_KEY = "opencut-last-visit";
const BACKUPS_KEY = "opencut-backups";

function readJson<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
}

function writeJson(key: string, value: unknown): void {
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Storage full or blocked: losing a timestamp is harmless.
	}
}

/** Notes this visit; returns milliseconds since the previous one (null the first time). */
export function recordVisit({ now = Date.now() }: { now?: number } = {}): number | null {
	const previous = readJson<number | null>(LAST_VISIT_KEY, null);
	writeJson(LAST_VISIT_KEY, now);
	return typeof previous === "number" ? Math.max(0, now - previous) : null;
}

export function readBackups(): Record<string, number> {
	return readJson<Record<string, number>>(BACKUPS_KEY, {});
}

export function recordBackup({ projectId, at = Date.now() }: { projectId: string; at?: number }): void {
	writeJson(BACKUPS_KEY, { ...readBackups(), [projectId]: at });
}

export function forgetBackup({ projectId }: { projectId: string }): void {
	const backups = readBackups();
	delete backups[projectId];
	writeJson(BACKUPS_KEY, backups);
}

export function isStandalone(): boolean {
	if (typeof window === "undefined") return false;
	return (
		window.matchMedia?.("(display-mode: standalone)").matches === true ||
		(navigator as Navigator & { standalone?: boolean }).standalone === true
	);
}

export async function readPersisted(): Promise<boolean | null> {
	if (typeof navigator === "undefined" || typeof navigator.storage?.persisted !== "function") return null;
	try {
		return await navigator.storage.persisted();
	} catch {
		return null;
	}
}

/** Asks the browser to keep this site's storage; returns whether it now does. */
export async function requestPersistence(): Promise<boolean | null> {
	if (typeof navigator === "undefined" || typeof navigator.storage?.persist !== "function") return null;
	try {
		return await navigator.storage.persist();
	} catch {
		return false;
	}
}

let visitGap: number | null | undefined;

/** The current assessment for this tab (records the visit once per load). */
export async function readDurability(): Promise<DurabilityAssessment & { persisted: boolean | null; browser: BrowserFamily; standalone: boolean }> {
	if (visitGap === undefined) visitGap = recordVisit();
	const persisted = await readPersisted();
	const browser = detectBrowserFamily({ userAgent: navigator.userAgent, vendor: navigator.vendor });
	const standalone = isStandalone();
	return { ...assessDurability({ persisted, browser, standalone, sinceLastVisitMs: visitGap }), persisted, browser, standalone };
}

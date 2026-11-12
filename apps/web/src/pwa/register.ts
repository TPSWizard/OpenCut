/**
 * Registers the service worker (production only), offers a reload when a
 * new build is waiting, and keeps the browser's install prompt for the
 * Install button. The worker itself is `src/pwa/sw.js`, emitted as
 * `/sw.js` by the Vite plugin in `vite.config.ts`.
 */

import { toast } from "sonner";
import { t } from "@/i18n";
import { create } from "zustand";

/** The `beforeinstallprompt` event, which browsers that support installing fire once. */
export interface InstallPromptEvent extends Event {
	prompt: () => Promise<void>;
	userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

interface PwaState {
	/** Kept until used; `null` when the browser offers no install or the app is installed. */
	installPrompt: InstallPromptEvent | null;
	installed: boolean;
	updateReady: boolean;
	setInstallPrompt: (event: InstallPromptEvent | null) => void;
	setInstalled: (installed: boolean) => void;
	setUpdateReady: (ready: boolean) => void;
}

export const usePwaStore = create<PwaState>()((set) => ({
	installPrompt: null,
	installed: false,
	updateReady: false,
	setInstallPrompt: (installPrompt) => set({ installPrompt }),
	setInstalled: (installed) => set((state) => ({ installed, installPrompt: installed ? null : state.installPrompt })),
	setUpdateReady: (updateReady) => set({ updateReady }),
}));

let registered = false;
let reloading = false;

function offerUpdate({ registration }: { registration: ServiceWorkerRegistration }): void {
	usePwaStore.getState().setUpdateReady(true);
	toast(t("A new version of ConzaCut is ready"), {
		id: "pwa-update",
		duration: Number.POSITIVE_INFINITY,
		description: t("Reload to use it; your projects stay as they are."),
		action: {
			label: t("Reload"),
			onClick: () => {
				registration.waiting?.postMessage({ type: "SKIP_WAITING" });
			},
		},
	});
}

/** Registers `/sw.js` once; safe to call on every mount. */
export async function registerServiceWorker(): Promise<void> {
	if (registered || typeof window === "undefined" || !("serviceWorker" in navigator)) return;
	if (!import.meta.env.PROD) return;
	registered = true;
	try {
		const registration = await navigator.serviceWorker.register("/sw.js", { scope: "/" });
		if (registration.waiting && navigator.serviceWorker.controller) {
			offerUpdate({ registration });
		}
		registration.addEventListener("updatefound", () => {
			const worker = registration.installing;
			if (!worker) return;
			worker.addEventListener("statechange", () => {
				if (worker.state === "installed" && navigator.serviceWorker.controller) {
					offerUpdate({ registration });
				}
			});
		});
		navigator.serviceWorker.addEventListener("controllerchange", () => {
			if (reloading) return;
			reloading = true;
			window.location.reload();
		});
		document.addEventListener("visibilitychange", () => {
			if (document.visibilityState === "visible") {
				registration.update().catch(() => {});
			}
		});
	} catch (error) {
		console.warn("[pwa] service worker registration failed", error);
	}
}

/** Keeps the browser's install prompt and notes when the app gets installed. */
export function watchInstallPrompt(): () => void {
	if (typeof window === "undefined") return () => {};
	const store = usePwaStore.getState();
	const onPrompt = (event: Event) => {
		event.preventDefault();
		store.setInstallPrompt(event as InstallPromptEvent);
	};
	const onInstalled = () => {
		usePwaStore.setState({ installed: true, installPrompt: null });
	};
	window.addEventListener("beforeinstallprompt", onPrompt);
	window.addEventListener("appinstalled", onInstalled);
	if (window.matchMedia?.("(display-mode: standalone)").matches) {
		usePwaStore.setState({ installed: true });
	}
	return () => {
		window.removeEventListener("beforeinstallprompt", onPrompt);
		window.removeEventListener("appinstalled", onInstalled);
	};
}

/** Shows the browser's install dialog; resolves to whether the user accepted. */
export async function promptInstall(): Promise<boolean> {
	const event = usePwaStore.getState().installPrompt;
	if (!event) return false;
	await event.prompt();
	const choice = await event.userChoice;
	usePwaStore.setState({ installPrompt: null, installed: choice.outcome === "accepted" });
	return choice.outcome === "accepted";
}

/** Files the OS opened the app with (the `.opencut.zip` file handler). */
export interface LaunchParams {
	files: FileSystemFileHandle[];
}

/**
 * Hands files the app was launched with to `onFiles`; browsers without the
 * launch queue do nothing. Returns whether the queue exists.
 */
export function consumeLaunchQueue({ onFiles }: { onFiles: (files: File[]) => void }): boolean {
	if (typeof window === "undefined") return false;
	const queue = (window as unknown as { launchQueue?: { setConsumer: (consumer: (params: LaunchParams) => void) => void } }).launchQueue;
	if (!queue) return false;
	queue.setConsumer((params) => {
		void (async () => {
			const files: File[] = [];
			for (const handle of params.files ?? []) {
				try {
					files.push(await handle.getFile());
				} catch {
					// A handle the OS could not open is skipped.
				}
			}
			if (files.length > 0) onFiles(files);
		})();
	});
	return true;
}

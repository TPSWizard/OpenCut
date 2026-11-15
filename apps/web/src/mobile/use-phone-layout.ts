import { useEffect, useState } from "react";
import { create } from "zustand";

/** "phone" or "desktop" when the person chose one; absent means automatic. */
const STORAGE_KEY = "conzacut-editor-layout";
/** Phones, and touch tablets up to a small landscape tablet. */
export const PHONE_LAYOUT_QUERY =
	"(max-width: 767px), (pointer: coarse) and (max-width: 1100px)";

export type LayoutPreference = "phone" | "desktop" | null;

function readPreference(): LayoutPreference {
	try {
		const value = localStorage.getItem(STORAGE_KEY);
		return value === "phone" || value === "desktop" ? value : null;
	} catch {
		return null;
	}
}

interface LayoutStore {
	preference: LayoutPreference;
	loaded: boolean;
	load: () => void;
	setPreference: (preference: LayoutPreference) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
	preference: null,
	loaded: false,
	load: () => set({ preference: readPreference(), loaded: true }),
	setPreference: (preference) => {
		try {
			if (preference) localStorage.setItem(STORAGE_KEY, preference);
			else localStorage.removeItem(STORAGE_KEY);
		} catch {
			// Private windows may refuse storage; the choice lasts this visit.
		}
		set({ preference, loaded: true });
	},
}));

/** Whether the screen is phone-sized (null before the first check). */
function useSmallScreen(): boolean | null {
	const [small, setSmall] = useState<boolean | null>(null);
	useEffect(() => {
		const query = window.matchMedia(PHONE_LAYOUT_QUERY);
		const update = () => setSmall(query.matches);
		update();
		query.addEventListener("change", update);
		return () => query.removeEventListener("change", update);
	}, []);
	return small;
}

/**
 * Whether the editor shows its phone layout: the person's choice (kept in
 * this browser), else the screen size. Null until both are known.
 */
export function usePhoneLayout(): {
	phone: boolean | null;
	setPreference: (preference: LayoutPreference) => void;
} {
	const { preference, loaded, load, setPreference } = useLayoutStore();
	const small = useSmallScreen();
	useEffect(() => {
		if (!loaded) load();
	}, [loaded, load]);
	const phone =
		small === null || !loaded ? null : preference ? preference === "phone" : small;
	return { phone, setPreference };
}

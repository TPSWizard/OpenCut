import { create } from "zustand";

/** Proxies being made (progress 0–1) and the last failure, by media id. */
interface ProxyJobsState {
	making: Record<string, number>;
	failed: Record<string, string>;
	start: (mediaId: string) => void;
	progress: (mediaId: string, progress: number) => void;
	finish: (mediaId: string, error?: string) => void;
}

export const useProxyJobs = create<ProxyJobsState>()((set) => ({
	making: {},
	failed: {},
	start: (mediaId) =>
		set((state) => {
			const failed = { ...state.failed };
			delete failed[mediaId];
			return { making: { ...state.making, [mediaId]: 0 }, failed };
		}),
	progress: (mediaId, progress) =>
		set((state) =>
			mediaId in state.making ? { making: { ...state.making, [mediaId]: progress } } : state,
		),
	finish: (mediaId, error) =>
		set((state) => {
			const making = { ...state.making };
			delete making[mediaId];
			return error ? { making, failed: { ...state.failed, [mediaId]: error } } : { making };
		}),
}));

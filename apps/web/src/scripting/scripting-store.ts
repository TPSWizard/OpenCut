import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DEFAULT_SCRIPT } from "./examples";
import type { ScriptLogEntry } from "./script-runner";

const MAX_LOG_ENTRIES = 500;

interface ScriptingState {
	code: string;
	logs: ScriptLogEntry[];
	isRunning: boolean;
	setCode: ({ code }: { code: string }) => void;
	appendLog: ({ entry }: { entry: ScriptLogEntry }) => void;
	clearLogs: () => void;
	setRunning: ({ isRunning }: { isRunning: boolean }) => void;
}

export const useScriptingStore = create<ScriptingState>()(
	persist(
		(set) => ({
			code: DEFAULT_SCRIPT,
			logs: [],
			isRunning: false,
			setCode: ({ code }) => set({ code }),
			appendLog: ({ entry }) =>
				set((state) => ({
					logs: [...state.logs, entry].slice(-MAX_LOG_ENTRIES),
				})),
			clearLogs: () => set({ logs: [] }),
			setRunning: ({ isRunning }) => set({ isRunning }),
		}),
		{
			name: "opencut-scripting",
			partialize: (state) => ({ code: state.code }),
		},
	),
);

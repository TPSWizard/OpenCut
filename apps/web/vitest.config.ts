import { defineConfig } from "vitest/config";
import viteReact from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";

// Vitest gets its own config: the Cloudflare plugin used for the app build
// rejects Vitest's environment options, and tests never need the worker.
export default defineConfig({
	resolve: { tsconfigPaths: true },
	plugins: [wasm(), viteReact()],
	test: {
		include: ["src/**/*.test.{ts,tsx}"],
		environment: "jsdom",
		setupFiles: ["./vitest.setup.ts"],
		globals: false,
	},
});

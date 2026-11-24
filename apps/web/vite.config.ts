import { defineConfig } from "vite";
import { devtools } from "@tanstack/devtools-vite";

import { tanstackStart } from "@tanstack/react-start/plugin/vite";

import viteReact from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { cloudflare } from "@cloudflare/vite-plugin";
import wasm from "vite-plugin-wasm";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import type { Plugin } from "vite";

/**
 * Emits `src/pwa/sw.js` as `/sw.js` in the client build with the build
 * stamped in, so every deploy ships a new worker (and a fresh cache).
 */
function serviceWorker(): Plugin {
	const buildId = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
	const source = fileURLToPath(new URL("./src/pwa/sw.js", import.meta.url));
	return {
		name: "conzacut-service-worker",
		apply: "build",
		generateBundle() {
			if (this.environment?.name && this.environment.name !== "client") return;
			this.emitFile({
				type: "asset",
				fileName: "sw.js",
				source: readFileSync(source, "utf8").replace("__BUILD_ID__", buildId),
			});
		},
	};
}

/** MediaPipe's wasm files, served from this site (the package exports no subpaths). */
const mediapipeWasm = join(dirname(createRequire(import.meta.url).resolve("@mediapipe/tasks-vision")), "wasm");

const config = defineConfig({
	resolve: { tsconfigPaths: true, alias: { "@mediapipe-wasm": mediapipeWasm } },
	// The dev pre-bundler must not wrap MediaPipe's loader: imported with
	// `?url`, it has to stay a file the face and person models load.
	optimizeDeps: { exclude: ["@mediapipe-wasm/vision_wasm_internal.js?url", "@mediapipe-wasm/vision_wasm_internal.js"] },
	plugins: [
		devtools(),
		// `opencut-wasm` is a wasm-pack "bundler" build: it imports its .wasm as an ES module.
		wasm(),
		serviceWorker(),
		cloudflare({ viteEnvironment: { name: "ssr" } }),
		tailwindcss(),
		tanstackStart(),
		viteReact(),
	],
	build: {
		target: "esnext",
	},
});

export default config;

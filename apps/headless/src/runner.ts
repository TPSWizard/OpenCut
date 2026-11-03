import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:net";
import { homedir } from "node:os";
import { dirname, extname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, type BrowserContext, type Page } from "playwright";
import type { HeadlessJob, HeadlessTimelineStep } from "./spec.ts";

export interface HeadlessOptions {
	/** Origin of a running OpenCut web app. When omitted, `vite preview` of apps/web is started. */
	url?: string;
	/** Show the browser window (debugging). */
	headed?: boolean;
	/** Persistent browser profile; projects are stored in it between runs. */
	profileDir?: string;
	/**
	 * Port for the local preview server (default 4173). Browser storage is
	 * per origin, so the port has to stay the same for projects to persist
	 * between runs; when it is busy a free one is used and a warning logged.
	 */
	port?: number;
	/** Per-step timeout in milliseconds. */
	timeoutMs?: number;
	log?: (line: string) => void;
}

export interface TimelineInfo {
	sceneId: string;
	duration: number;
	tracks: Array<{
		id: string;
		name: string;
		type: string;
		lane: string;
		elements: Array<{
			id: string;
			type: string;
			name: string;
			startTime: number;
			duration: number;
			mediaId?: string;
		}>;
	}>;
}

const MEDIA_ORIGIN = "https://media.opencut.local";

const MIME_BY_EXTENSION: Record<string, string> = {
	".mp4": "video/mp4",
	".m4v": "video/mp4",
	".mov": "video/quicktime",
	".webm": "video/webm",
	".mkv": "video/x-matroska",
	".mp3": "audio/mpeg",
	".wav": "audio/wav",
	".m4a": "audio/mp4",
	".aac": "audio/aac",
	".ogg": "audio/ogg",
	".flac": "audio/flac",
	".png": "image/png",
	".jpg": "image/jpeg",
	".jpeg": "image/jpeg",
	".gif": "image/gif",
	".webp": "image/webp",
	".avif": "image/avif",
	".svg": "image/svg+xml",
};

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const webAppDir = resolve(repoRoot, "apps/web");

export function defaultProfileDir(): string {
	return resolve(homedir(), ".opencut", "headless-profile");
}

/** The preview server's default port; stable so stored projects persist. */
export const DEFAULT_PREVIEW_PORT = 4173;

function portIsFree(port: number): Promise<boolean> {
	return new Promise((resolveFree) => {
		const server = createServer();
		server.unref();
		server.once("error", () => resolveFree(false));
		// No host: bind every interface, as vite preview does, so an IPv6-only
		// listener on the port is seen too.
		server.listen(port, () => server.close(() => resolveFree(true)));
	});
}

async function freePort(): Promise<number> {
	return new Promise((resolvePort, reject) => {
		const server = createServer();
		server.unref();
		server.on("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			const port = typeof address === "object" && address ? address.port : 0;
			server.close(() => resolvePort(port));
		});
	});
}

async function waitForHttp(url: string, timeoutMs: number): Promise<void> {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		try {
			const response = await fetch(url);
			if (response.ok) return;
		} catch {
			// not up yet
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error(`${url} did not come up within ${timeoutMs} ms`);
}

/**
 * Serves the production build of apps/web with `vite preview`. Builds it
 * first when `dist/` is missing.
 */
export class LocalWebServer {
	private child: ChildProcess | null = null;
	url = "";

	constructor(
		private readonly log: (line: string) => void,
		private readonly preferredPort: number = DEFAULT_PREVIEW_PORT,
	) {}

	async start(): Promise<string> {
		const distIndex = resolve(webAppDir, "dist/client/index.html");
		const serverEntry = resolve(webAppDir, "dist/server/index.js");
		if (!existsSync(serverEntry) && !existsSync(distIndex)) {
			this.log("apps/web has no production build yet; running `bun run build` (this takes a minute)");
			await runCommand("bun", ["run", "build"], webAppDir);
		}
		let port = this.preferredPort;
		if (!(await portIsFree(port))) {
			port = await freePort();
			this.log(
				`port ${this.preferredPort} is busy; using ${port}. Browser storage is per origin, so projects saved under another port will not show up here.`,
			);
		}
		this.url = `http://localhost:${port}`;
		this.log(`starting vite preview on ${this.url}`);
		this.child = spawn("bun", ["run", "preview", "--", "--port", String(port), "--strictPort"], {
			cwd: webAppDir,
			stdio: ["ignore", "pipe", "pipe"],
			shell: process.platform === "win32",
		});
		this.child.stderr?.on("data", (chunk: Buffer) => {
			const text = chunk.toString().trim();
			if (text) this.log(`[preview] ${text}`);
		});
		await waitForHttp(this.url, 60_000);
		return this.url;
	}

	/** Stops the server and waits until it is gone, so the port is free for the next run. */
	async stop(): Promise<void> {
		if (!this.child) return;
		const child = this.child;
		this.child = null;
		const exited = new Promise<void>((resolveExit) => {
			if (child.exitCode !== null || child.signalCode !== null) {
				resolveExit();
				return;
			}
			child.once("exit", () => resolveExit());
			setTimeout(resolveExit, 10_000).unref();
		});
		if (process.platform === "win32" && child.pid) {
			await new Promise<void>((resolveKill) => {
				const killer = spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
				killer.once("exit", () => resolveKill());
				killer.once("error", () => resolveKill());
			});
		} else {
			child.kill("SIGTERM");
		}
		await exited;
	}
}

function runCommand(command: string, args: string[], cwd: string): Promise<void> {
	return new Promise((resolveRun, reject) => {
		const child = spawn(command, args, {
			cwd,
			stdio: "inherit",
			shell: process.platform === "win32",
		});
		child.on("exit", (code) => {
			if (code === 0) resolveRun();
			else reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
		});
		child.on("error", reject);
	});
}

/**
 * Drives the OpenCut editor in a headless browser through the Editor API
 * (`window.opencut`). One runner can process many jobs.
 */
export class HeadlessRunner {
	private context: BrowserContext | null = null;
	private page: Page | null = null;
	private localServer: LocalWebServer | null = null;
	private baseUrl = "";
	private readonly log: (line: string) => void;
	private readonly timeoutMs: number;
	private readonly mediaFiles = new Map<string, string>();

	constructor(private readonly options: HeadlessOptions = {}) {
		this.log = options.log ?? ((line) => console.error(line));
		this.timeoutMs = options.timeoutMs ?? 180_000;
	}

	get url(): string {
		return this.baseUrl;
	}

	async start(): Promise<void> {
		if (this.context) return;
		if (this.options.url) {
			this.baseUrl = this.options.url.replace(/\/+$/, "");
		} else {
			this.localServer = new LocalWebServer(this.log, this.options.port);
			this.baseUrl = await this.localServer.start();
		}

		const profileDir = this.options.profileDir ?? defaultProfileDir();
		mkdirSync(profileDir, { recursive: true });
		this.context = await chromium.launchPersistentContext(profileDir, {
			headless: !this.options.headed,
			acceptDownloads: true,
			viewport: { width: 1600, height: 900 },
			args: [
				"--enable-unsafe-webgpu",
				"--ignore-gpu-blocklist",
				"--autoplay-policy=no-user-gesture-required",
			],
		});
		this.context.setDefaultTimeout(this.timeoutMs);

		await this.context.route(`${MEDIA_ORIGIN}/**`, async (route) => {
			const url = new URL(route.request().url());
			const alias = decodeURIComponent(url.pathname.split("/")[1] ?? "");
			const filePath = this.mediaFiles.get(alias);
			if (!filePath) {
				await route.fulfill({ status: 404, body: `unknown media alias ${alias}` });
				return;
			}
			await route.fulfill({
				status: 200,
				body: readFileSync(filePath),
				headers: {
					"content-type": MIME_BY_EXTENSION[extname(filePath).toLowerCase()] ?? "application/octet-stream",
					"content-length": String(statSync(filePath).size),
				},
			});
		});

		this.page = this.context.pages()[0] ?? (await this.context.newPage());
		this.page.on("console", (message) => {
			if (message.type() === "error" || message.type() === "warning") {
				this.log(`[browser:${message.type()}] ${message.text()}`);
			}
		});
		this.page.on("pageerror", (error) => this.log(`[browser:pageerror] ${error.message}`));
	}

	async stop(): Promise<void> {
		await this.context?.close();
		this.context = null;
		this.page = null;
		await this.localServer?.stop();
		this.localServer = null;
	}

	private requirePage(): Page {
		if (!this.page) throw new Error("runner not started");
		return this.page;
	}

	private async waitForApi({ needProject }: { needProject: boolean }): Promise<void> {
		const page = this.requirePage();
		await page.waitForFunction(
			(wantProject) => {
				const bridge = (window as unknown as { opencut?: { api?: { dispatch: unknown } } }).opencut;
				if (!bridge?.api) return false;
				if (!wantProject) return true;
				const w = window as unknown as {
					__opencutReady?: boolean;
					opencut: { call: (method: string, params?: unknown) => Promise<{ ready: boolean }> };
				};
				void w.opencut.call("app.info").then((info) => {
					w.__opencutReady = info.ready;
				});
				return w.__opencutReady === true;
			},
			needProject,
			{ timeout: this.timeoutMs, polling: 200 },
		);
	}

	/** Call any Editor API method in the page. */
	async call<T = unknown>(method: string, params: unknown = {}): Promise<T> {
		const page = this.requirePage();
		return page.evaluate(
			async ([m, p]) => {
				const w = window as unknown as {
					opencut: { call: (method: string, params?: unknown) => Promise<unknown> };
				};
				return w.opencut.call(m as string, p);
			},
			[method, params] as const,
		) as Promise<T>;
	}

	async listProjects(): Promise<Array<{ id: string; name: string; duration: number; updatedAt: string }>> {
		await this.gotoProjects();
		return this.call("project.list");
	}

	private async gotoProjects(): Promise<void> {
		const page = this.requirePage();
		await page.goto(`${this.baseUrl}/projects`, { waitUntil: "domcontentloaded" });
		await this.waitForApi({ needProject: false });
	}

	/** Open an existing project, or create a new one when `projectId` is omitted. */
	async openProject({ projectId, name }: { projectId?: string; name?: string }): Promise<string> {
		const page = this.requirePage();
		let id = projectId;
		if (!id) {
			await this.gotoProjects();
			const created = await this.call<{ id: string; name: string }>("project.create", { name });
			id = created.id;
			this.log(`created project "${created.name}" (${id})`);
		}
		await page.goto(`${this.baseUrl}/editor/${id}`, { waitUntil: "domcontentloaded" });
		await page.evaluate(() => {
			(window as unknown as { __opencutReady?: boolean }).__opencutReady = undefined;
		});
		await this.waitForApi({ needProject: true });
		// Dismiss onboarding so it never intercepts anything.
		await page.evaluate(() => {
			try {
				localStorage.setItem("onboarding-completed", "true");
			} catch {
				// ignore
			}
		});
		return id;
	}

	async applyProjectSettings(settings: NonNullable<HeadlessJob["project"]>): Promise<void> {
		const { name, ...rest } = settings;
		if (name) await this.call("project.rename", { name });
		if (rest.fps !== undefined || rest.canvas || rest.background) {
			await this.call("project.updateSettings", rest);
		}
	}

	/** Import media files from disk (or URLs) and return alias → media id. */
	async importMedia({
		media,
		baseDir,
	}: {
		media: Record<string, string>;
		baseDir: string;
	}): Promise<Record<string, string>> {
		const ids: Record<string, string> = {};
		for (const [alias, location] of Object.entries(media)) {
			let url = location;
			let name: string | undefined;
			if (!/^https?:\/\//i.test(location)) {
				const filePath = isAbsolute(location) ? location : resolve(baseDir, location);
				if (!existsSync(filePath)) throw new Error(`media "${alias}": ${filePath} does not exist`);
				this.mediaFiles.set(alias, filePath);
				name = filePath.split(/[\\/]/).pop();
				url = `${MEDIA_ORIGIN}/${encodeURIComponent(alias)}/${encodeURIComponent(name ?? alias)}`;
			}
			this.log(`importing ${alias} ← ${location}`);
			const asset = await this.call<{ id: string; name: string; duration?: number }>("media.importUrl", {
				url,
				name,
			});
			ids[alias] = asset.id;
			this.log(`  → ${asset.name}${asset.duration ? ` (${asset.duration.toFixed(2)} s)` : ""}`);
		}
		return ids;
	}

	async applyTimeline({
		steps,
		mediaIds,
	}: {
		steps: HeadlessTimelineStep[];
		mediaIds: Record<string, string>;
	}): Promise<void> {
		for (const step of steps) {
			if (step.type === "clip") {
				const mediaId = mediaIds[step.media];
				if (!mediaId) throw new Error(`no imported media for alias "${step.media}"`);
				const element = await this.call<{ id: string; startTime: number; duration: number }>(
					"timeline.addClip",
					{
						mediaId,
						at: step.at ?? 0,
						duration: step.duration,
						trackType: step.trackType,
						name: step.name,
					},
				);
				this.log(`clip ${step.media} @ ${element.startTime.toFixed(2)}s for ${element.duration.toFixed(2)}s`);
			} else {
				const element = await this.call<{ id: string; startTime: number; duration: number }>(
					"timeline.addText",
					{
						text: step.text,
						at: step.at ?? 0,
						duration: step.duration,
						name: step.name,
						params: step.params,
					},
				);
				this.log(`text "${step.text}" @ ${element.startTime.toFixed(2)}s for ${element.duration.toFixed(2)}s`);
			}
		}
	}

	/** Run user code in the editor page with `opencut` bound to the Editor API. */
	async runScript({ code }: { code: string }): Promise<unknown> {
		const page = this.requirePage();
		const onConsole = (message: import("playwright").ConsoleMessage) => {
			if (message.type() === "log" || message.type() === "info") {
				this.log(`[script] ${message.text()}`);
			}
		};
		page.on("console", onConsole);
		try {
			return await page.evaluate(async (source) => {
				const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
					...args: string[]
				) => (...values: unknown[]) => Promise<unknown>;
				const fn = new AsyncFunction("opencut", "sleep", source);
				const w = window as unknown as { opencut: unknown };
				return fn(w.opencut, (ms: number) => new Promise((r) => setTimeout(r, ms)));
			}, code);
		} finally {
			page.off("console", onConsole);
		}
	}

	async getTimeline(): Promise<TimelineInfo> {
		return this.call<TimelineInfo>("timeline.get");
	}

	/**
	 * Import a project bundle (`.opencut.zip`, from the web app's "Export
	 * project…" or the desktop editor's "Save bundle…") through the projects
	 * page, and open it. Returns the project id.
	 */
	async importBundle({ path }: { path: string }): Promise<string> {
		const page = this.requirePage();
		await this.gotoProjects();
		const input = page.locator('input[type="file"][accept*="zip"]');
		await input.waitFor({ state: "attached", timeout: this.timeoutMs });
		await input.setInputFiles(path);
		// The page either opens the editor or shows the failure as a toast.
		const failure = page.getByText("Failed to import project");
		const outcome = await Promise.race([
			page.waitForURL(/\/editor\//, { timeout: this.timeoutMs }).then(() => "opened" as const),
			failure.waitFor({ state: "visible", timeout: this.timeoutMs }).then(() => "failed" as const),
		]);
		if (outcome === "failed") {
			const toast = await failure.locator("..").innerText().catch(() => "");
			const reason = toast.replace(/\s+/g, " ").replace("Failed to import project", "").trim();
			throw new Error(`import failed${reason ? `: ${reason}` : ""}`);
		}
		await page.evaluate(() => {
			(window as unknown as { __opencutReady?: boolean }).__opencutReady = undefined;
		});
		await this.waitForApi({ needProject: true });
		const id = page.url().split("/editor/")[1]?.split(/[?#]/)[0] ?? "";
		this.log(`imported bundle as project ${id}`);
		return id;
	}

	/** Export the open project and its media as a bundle to `output`. Returns the written path. */
	async exportBundle({ output }: { output: string }): Promise<string> {
		const page = this.requirePage();
		const [download] = await Promise.all([
			page.waitForEvent("download", { timeout: this.timeoutMs }),
			this.call("project.exportBundle", {}),
		]);
		mkdirSync(dirname(output), { recursive: true });
		await download.saveAs(output);
		return output;
	}

	/** Render the active scene and save it to `output`. Returns the written path. */
	async exportTo({
		output,
		format,
		quality,
		includeAudio,
		fps,
	}: {
		output: string;
		format?: "mp4" | "webm";
		quality?: string;
		includeAudio?: boolean;
		fps?: number;
	}): Promise<string> {
		const page = this.requirePage();
		const chosenFormat = format ?? (output.toLowerCase().endsWith(".webm") ? "webm" : "mp4");
		const target = extname(output) ? output : `${output}.${chosenFormat}`;
		mkdirSync(dirname(target), { recursive: true });
		this.log(`exporting ${chosenFormat} (${quality ?? "high"})…`);
		const started = Date.now();
		const downloadPromise = page.waitForEvent("download", { timeout: this.timeoutMs * 4 });
		const resultPromise = this.call<{ fileName: string; bytes: number }>("export.download", {
			format: chosenFormat,
			quality,
			includeAudio,
			fps,
		});
		const [download, result] = await Promise.all([downloadPromise, resultPromise]);
		await download.saveAs(target);
		this.log(
			`wrote ${target} (${(result.bytes / 1024).toFixed(0)} KB) in ${((Date.now() - started) / 1000).toFixed(1)} s`,
		);
		return target;
	}

	/** Run a whole job file end to end. Returns the exported path, if any. */
	async runJob({ job, baseDir }: { job: HeadlessJob; baseDir: string }): Promise<string | null> {
		await this.start();
		await this.openProject({ projectId: job.projectId, name: job.project?.name });
		if (job.project) await this.applyProjectSettings(job.project);
		const mediaIds = job.media ? await this.importMedia({ media: job.media, baseDir }) : {};
		if (job.timeline) await this.applyTimeline({ steps: job.timeline, mediaIds });
		if (job.script) {
			const scriptPath = isAbsolute(job.script) ? job.script : resolve(baseDir, job.script);
			const code = existsSync(scriptPath) ? readFileSync(scriptPath, "utf8") : job.script;
			const result = await this.runScript({ code });
			if (result !== undefined) this.log(`[script] → ${JSON.stringify(result)}`);
		}
		await this.call("project.save");
		if (job.export) {
			const output = isAbsolute(job.export.output)
				? job.export.output
				: resolve(baseDir, job.export.output);
			return this.exportTo({ ...job.export, output });
		}
		return null;
	}
}

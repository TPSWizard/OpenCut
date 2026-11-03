#!/usr/bin/env bun
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { parseArgs } from "node:util";
import { HeadlessRunner, defaultProfileDir } from "./runner.ts";
import { parseHeadlessJob } from "./spec.ts";

const USAGE = `opencut-headless — drive OpenCut without a UI

Usage:
  opencut-headless render <job.json> [<job2.json> ...]   build and export each job
  opencut-headless run <script.js> [--project <id>]      run a script against a project
  opencut-headless inspect --project <id>                 print the timeline as JSON
  opencut-headless list                                   list projects in the profile
  opencut-headless import <bundle.opencut.zip>            import a project bundle, print its id
  opencut-headless export --project <id> --output <path>  save a project as a bundle

Options:
  --url <origin>       use a running OpenCut web app instead of starting one
  --profile <dir>      browser profile holding the projects (default: ${defaultProfileDir()})
  --port <port>        port of the local preview server (default 4173; keep it fixed so projects persist)
  --headed             show the browser window
  --timeout <ms>       per-step timeout (default 180000)
  --output <path>      (run) export after the script finishes
  --new [name]         (run) create a project first
  -h, --help           this text

Job files are documented in apps/headless/README.md.`;

function fail(message: string): never {
	console.error(message);
	process.exit(1);
}

async function main() {
	const { values, positionals } = parseArgs({
		args: process.argv.slice(2),
		allowPositionals: true,
		options: {
			url: { type: "string" },
			profile: { type: "string" },
			port: { type: "string" },
			headed: { type: "boolean", default: false },
			timeout: { type: "string" },
			project: { type: "string" },
			output: { type: "string" },
			new: { type: "string" },
			help: { type: "boolean", short: "h", default: false },
		},
	});

	const [command, ...rest] = positionals;
	if (values.help || !command) {
		console.log(USAGE);
		process.exit(values.help ? 0 : 1);
	}

	const runner = new HeadlessRunner({
		url: values.url,
		profileDir: values.profile,
		port: values.port ? Number(values.port) : undefined,
		headed: values.headed,
		timeoutMs: values.timeout ? Number(values.timeout) : undefined,
	});

	const shutdown = async (code: number) => {
		await runner.stop().catch(() => {});
		process.exit(code);
	};
	process.on("SIGINT", () => void shutdown(130));

	try {
		switch (command) {
			case "render": {
				if (rest.length === 0) fail("render needs at least one job file");
				await runner.start();
				for (const jobPath of rest) {
					const absolute = resolve(jobPath);
					if (!existsSync(absolute)) fail(`${absolute} does not exist`);
					const job = parseHeadlessJob(JSON.parse(readFileSync(absolute, "utf8")));
					console.error(`▶ ${jobPath}`);
					const output = await runner.runJob({ job, baseDir: dirname(absolute) });
					if (output) console.log(output);
				}
				break;
			}
			case "run": {
				const [scriptPath] = rest;
				if (!scriptPath) fail("run needs a script file");
				const absolute = resolve(scriptPath);
				if (!existsSync(absolute)) fail(`${absolute} does not exist`);
				const code = readFileSync(absolute, "utf8");
				await runner.start();
				if (!values.project && values.new === undefined) {
					fail("run needs --project <id> or --new [name]");
				}
				await runner.openProject({
					projectId: values.project,
					name: values.new && values.new.length > 0 ? values.new : undefined,
				});
				const result = await runner.runScript({ code });
				if (result !== undefined) console.log(JSON.stringify(result, null, 2));
				await runner.call("project.save");
				if (values.output) {
					const written = await runner.exportTo({ output: resolve(values.output) });
					console.log(written);
				}
				break;
			}
			case "inspect": {
				if (!values.project) fail("inspect needs --project <id>");
				await runner.start();
				await runner.openProject({ projectId: values.project });
				const timeline = await runner.getTimeline();
				console.log(JSON.stringify(timeline, null, 2));
				break;
			}
			case "import": {
				const [bundlePath] = rest;
				if (!bundlePath) fail("import needs a bundle file");
				const absolute = resolve(bundlePath);
				if (!existsSync(absolute)) fail(`${absolute} does not exist`);
				await runner.start();
				const id = await runner.importBundle({ path: absolute });
				console.log(id);
				break;
			}
			case "export": {
				if (!values.project) fail("export needs --project <id>");
				if (!values.output) fail("export needs --output <path>");
				await runner.start();
				await runner.openProject({ projectId: values.project });
				const written = await runner.exportBundle({ output: resolve(values.output) });
				console.log(written);
				break;
			}
			case "list": {
				await runner.start();
				const projects = await runner.listProjects();
				for (const project of projects) {
					console.log(`${project.id}\t${project.duration.toFixed(2)}s\t${project.updatedAt}\t${project.name}`);
				}
				if (projects.length === 0) console.error("no projects in this profile yet");
				break;
			}
			default:
				fail(`unknown command "${command}"\n\n${USAGE}`);
		}
		await shutdown(0);
	} catch (error) {
		console.error(error instanceof Error ? error.message : String(error));
		await shutdown(1);
	}
}

void main();

/**
 * Job file format for the headless runner.
 *
 * ```json
 * {
 *   "project": { "name": "Promo", "fps": 30, "canvas": { "width": 1920, "height": 1080 } },
 *   "media": { "intro": "./intro.mp4", "music": "./music.mp3" },
 *   "timeline": [
 *     { "type": "clip", "media": "intro", "at": 0 },
 *     { "type": "text", "text": "Hello", "at": 1, "duration": 3 },
 *     { "type": "clip", "media": "music", "at": 0, "trackType": "audio" }
 *   ],
 *   "script": "./finish.js",
 *   "export": { "format": "mp4", "quality": "high", "output": "./promo.mp4" }
 * }
 * ```
 *
 * Paths resolve relative to the job file. `script` runs in the editor page
 * with the Editor API available as `opencut` (same surface as the scripting
 * tab) after the timeline steps and before the export.
 */

export type HeadlessExportFormat = "mp4" | "webm";
export type HeadlessExportQuality = "low" | "medium" | "high" | "very_high";

export interface HeadlessProjectSpec {
	name?: string;
	fps?: number;
	canvas?: { width: number; height: number };
	background?:
		| { type: "color"; color: string }
		| { type: "blur"; blurIntensity: number };
}

export type HeadlessTimelineStep =
	| {
			type: "clip";
			media: string;
			at?: number;
			duration?: number;
			trackType?: "video" | "audio";
			name?: string;
	  }
	| {
			type: "text";
			text: string;
			at?: number;
			duration?: number;
			name?: string;
			params?: Record<string, number | string | boolean>;
	  };

export interface HeadlessExportSpec {
	format?: HeadlessExportFormat;
	quality?: HeadlessExportQuality;
	includeAudio?: boolean;
	fps?: number;
	/** Platform preset id (`export.presets`): forces MP4 with the platform's encoder settings. */
	preset?: string;
	/** Output file path; the extension is added when missing. */
	output: string;
}

export interface HeadlessJob {
	/** Open an existing project by id instead of creating one. */
	projectId?: string;
	project?: HeadlessProjectSpec;
	/** Media alias → file path (or http(s) URL). */
	media?: Record<string, string>;
	timeline?: HeadlessTimelineStep[];
	/** Path to a JavaScript file, or inline code. */
	script?: string;
	export?: HeadlessExportSpec;
}

export class HeadlessSpecError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "HeadlessSpecError";
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalNumber(value: unknown, field: string): number | undefined {
	if (value === undefined) return undefined;
	if (typeof value !== "number" || !Number.isFinite(value)) {
		throw new HeadlessSpecError(`${field} must be a number`);
	}
	return value;
}

function optionalString(value: unknown, field: string): string | undefined {
	if (value === undefined) return undefined;
	if (typeof value !== "string") {
		throw new HeadlessSpecError(`${field} must be a string`);
	}
	return value;
}

export function parseHeadlessJob(value: unknown): HeadlessJob {
	if (!isRecord(value)) throw new HeadlessSpecError("job must be a JSON object");

	const job: HeadlessJob = {};
	job.projectId = optionalString(value.projectId, "projectId");

	if (value.project !== undefined) {
		if (!isRecord(value.project)) {
			throw new HeadlessSpecError("project must be an object");
		}
		const project: HeadlessProjectSpec = {};
		project.name = optionalString(value.project.name, "project.name");
		project.fps = optionalNumber(value.project.fps, "project.fps");
		if (value.project.canvas !== undefined) {
			if (
				!isRecord(value.project.canvas) ||
				typeof value.project.canvas.width !== "number" ||
				typeof value.project.canvas.height !== "number"
			) {
				throw new HeadlessSpecError("project.canvas needs numeric width and height");
			}
			project.canvas = {
				width: value.project.canvas.width,
				height: value.project.canvas.height,
			};
		}
		if (value.project.background !== undefined) {
			project.background = value.project.background as HeadlessProjectSpec["background"];
		}
		job.project = project;
	}

	if (value.media !== undefined) {
		if (!isRecord(value.media)) {
			throw new HeadlessSpecError("media must map aliases to paths");
		}
		job.media = {};
		for (const [alias, path] of Object.entries(value.media)) {
			if (typeof path !== "string" || path.length === 0) {
				throw new HeadlessSpecError(`media.${alias} must be a path or URL`);
			}
			job.media[alias] = path;
		}
	}

	if (value.timeline !== undefined) {
		if (!Array.isArray(value.timeline)) {
			throw new HeadlessSpecError("timeline must be an array of steps");
		}
		job.timeline = value.timeline.map((step, index): HeadlessTimelineStep => {
			if (!isRecord(step)) {
				throw new HeadlessSpecError(`timeline[${index}] must be an object`);
			}
			const at = optionalNumber(step.at, `timeline[${index}].at`);
			const duration = optionalNumber(step.duration, `timeline[${index}].duration`);
			const name = optionalString(step.name, `timeline[${index}].name`);
			if (step.type === "clip") {
				if (typeof step.media !== "string") {
					throw new HeadlessSpecError(`timeline[${index}].media must name a media alias`);
				}
				if (!job.media || !(step.media in job.media)) {
					throw new HeadlessSpecError(
						`timeline[${index}] references unknown media "${step.media}"`,
					);
				}
				const trackType = optionalString(step.trackType, `timeline[${index}].trackType`);
				if (trackType !== undefined && trackType !== "video" && trackType !== "audio") {
					throw new HeadlessSpecError(`timeline[${index}].trackType must be video or audio`);
				}
				return { type: "clip", media: step.media, at, duration, name, trackType };
			}
			if (step.type === "text") {
				if (typeof step.text !== "string") {
					throw new HeadlessSpecError(`timeline[${index}].text must be a string`);
				}
				return {
					type: "text",
					text: step.text,
					at,
					duration,
					name,
					params: isRecord(step.params)
						? (step.params as Record<string, number | string | boolean>)
						: undefined,
				};
			}
			throw new HeadlessSpecError(`timeline[${index}].type must be "clip" or "text"`);
		});
	}

	job.script = optionalString(value.script, "script");

	if (value.export !== undefined) {
		if (!isRecord(value.export) || typeof value.export.output !== "string") {
			throw new HeadlessSpecError("export.output must be a file path");
		}
		const format = optionalString(value.export.format, "export.format");
		if (format !== undefined && format !== "mp4" && format !== "webm") {
			throw new HeadlessSpecError("export.format must be mp4 or webm");
		}
		const quality = optionalString(value.export.quality, "export.quality");
		if (
			quality !== undefined &&
			!["low", "medium", "high", "very_high"].includes(quality)
		) {
			throw new HeadlessSpecError("export.quality must be low, medium, high or very_high");
		}
		const preset = optionalString(value.export.preset, "export.preset");
		job.export = {
			output: value.export.output,
			format: format as HeadlessExportFormat | undefined,
			quality: quality as HeadlessExportQuality | undefined,
			preset,
			includeAudio:
				typeof value.export.includeAudio === "boolean"
					? value.export.includeAudio
					: undefined,
			fps: optionalNumber(value.export.fps, "export.fps"),
		};
	}

	if (!job.projectId && !job.project && !job.timeline && !job.script && !job.export) {
		throw new HeadlessSpecError("job is empty");
	}

	return job;
}

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { readProjectBundle } from "@/project/bundle";
import index from "../../../public/templates/index.json";
import { parseTemplateIndex } from "../templates";

const TEMPLATES_DIR = join(__dirname, "../../../public/templates");

describe("starter templates", () => {
	it("the shipped index lists every template with its bundle", () => {
		const templates = parseTemplateIndex({ json: index });
		expect(templates.map((template) => template.id)).toEqual([
			"vertical-hook",
			"landscape-title",
			"quote-card",
			"countdown",
			"lower-third",
		]);
		for (const template of templates) {
			expect(template.source).toBe("starter");
			expect(template.duration).toBeGreaterThan(0);
			expect(template.fonts.length).toBeGreaterThan(0);
		}
	});

	it("each bundle (written by the desktop crate) reads as a web project", async () => {
		for (const template of parseTemplateIndex({ json: index })) {
			const bytes = readFileSync(join(TEMPLATES_DIR, template.file!));
			const { project, media } = await readProjectBundle({ file: new Blob([bytes]) });
			expect(media).toHaveLength(0);
			expect(project.settings.canvasSize).toEqual({ width: template.width, height: template.height });
			const elements: Array<{ type: string }> = project.scenes[0].tracks.overlay.flatMap((track) => track.elements as Array<{ type: string }>);
			const texts = elements.filter((element) => element.type === "text");
			expect(texts.length).toBeGreaterThan(0);
		}
	});

	it("drops entries with unsafe ids or file names", () => {
		const templates = parseTemplateIndex({
			json: {
				format: "opencut-templates",
				templates: [
					{ id: "ok", file: "ok.opencut.zip", width: 10, height: 10 },
					{ id: "../x", file: "x.opencut.zip", width: 10, height: 10 },
					{ id: "y", file: "../../y.opencut.zip", width: 10, height: 10 },
					{ id: "z", file: "z.opencut.zip", width: 0, height: 10 },
				],
			},
		});
		expect(templates.map((template) => template.id)).toEqual(["ok"]);
		expect(parseTemplateIndex({ json: { format: "other", templates: [] } })).toEqual([]);
	});
});

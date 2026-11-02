export interface ScriptExample {
	id: string;
	title: string;
	code: string;
}

export const SCRIPT_EXAMPLES: ScriptExample[] = [
	{
		id: "inspect",
		title: "Inspect the project",
		code: `// The Editor API is available as \`opencut\`. Every call returns a promise.
const project = await opencut.project.get();
console.log("Project:", project.name, \`\${project.canvas.width}x\${project.canvas.height}\`, \`\${project.fps} fps\`);

const timeline = await opencut.timeline.get();
for (const track of timeline.tracks) {
  console.log(\`\${track.lane} \${track.type} track "\${track.name}": \${track.elements.length} element(s)\`);
  for (const el of track.elements) {
    console.log(\`  \${el.type} "\${el.name}" @ \${el.startTime.toFixed(2)}s for \${el.duration.toFixed(2)}s\`);
  }
}
return \`\${timeline.duration.toFixed(2)} s total\`;`,
	},
	{
		id: "sequence",
		title: "Lay all media out in sequence",
		code: `// Places every imported media asset one after another on the main track.
const media = await opencut.media.list();
let at = 0;
for (const asset of media) {
  if (asset.type === "audio") continue;
  const clip = await opencut.timeline.addClip({ mediaId: asset.id, at });
  at += clip.duration;
}
console.log(\`Placed \${media.length} asset(s), timeline is now \${at.toFixed(2)} s\`);`,
	},
	{
		id: "title",
		title: "Add a title card",
		code: `// Adds a 3 second text element at the playhead and centres it.
const title = await opencut.timeline.addText({
  text: "Hello from the scripting tab",
  duration: 3,
  params: { fontSize: 72 },
});
await opencut.selection.set({ elements: [title] });
console.log("Added", title.id);`,
	},
	{
		id: "ripple",
		title: "Close gaps on the main track",
		code: `// Moves every main-track clip so it starts where the previous one ends.
const timeline = await opencut.timeline.get();
const main = timeline.tracks.find((track) => track.lane === "main");
let cursor = 0;
for (const el of [...main.elements].sort((a, b) => a.startTime - b.startTime)) {
  if (Math.abs(el.startTime - cursor) > 1e-6) {
    await opencut.timeline.moveElement({ elementId: el.id, startTime: cursor });
  }
  cursor += el.duration;
}
console.log("Main track is gap-free:", cursor.toFixed(2), "s");`,
	},
	{
		id: "export",
		title: "Export an MP4",
		code: `// Renders the active scene and downloads it. This can take a while.
const result = await opencut.export.download({ format: "mp4", quality: "high" });
console.log(\`Saved \${result.fileName} (\${(result.bytes / 1024).toFixed(0)} KB)\`);`,
	},
];

export const DEFAULT_SCRIPT = SCRIPT_EXAMPLES[0]?.code ?? "";

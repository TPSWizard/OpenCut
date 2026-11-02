/**
 * Background entry of the sample plugin. It runs in a Worker for as long as
 * the project is open and receives the remote Editor API.
 *
 * This one keeps every text element's name in sync with its content, so the
 * timeline shows what a title says instead of "Text".
 */
export async function activate(opencut) {
	const sync = async () => {
		const timeline = await opencut.timeline.get();
		for (const track of timeline.tracks) {
			for (const element of track.elements) {
				if (element.type !== "text") continue;
				const content = String(element.params.content ?? "").trim();
				const wanted = content.slice(0, 32) || "Text";
				if (content && element.name !== wanted) {
					await opencut.timeline.updateElement({
						elementId: element.id,
						trackId: track.id,
						patch: { name: wanted },
					});
				}
			}
		}
	};

	let scheduled = false;
	const unsubscribe = opencut.on("timeline", () => {
		if (scheduled) return;
		scheduled = true;
		setTimeout(() => {
			scheduled = false;
			sync().catch(() => {});
		}, 250);
	});

	await sync();

	// Returning a function registers it as the deactivate hook.
	return () => unsubscribe();
}

import { toast } from "sonner";
import type { EditorCore } from "@/core";
import type { useT } from "@/i18n";
import { projectCreditsText } from "../project-assets";

/**
 * Copies the project's credit lines (from its asset manifest). The text is
 * handed to the clipboard as a promise, so the click still counts while the
 * licences are looked up.
 */
export function copyProjectCredits({ editor, t }: { editor: EditorCore; t: ReturnType<typeof useT> }): void {
	let empty = false;
	const text = projectCreditsText({ editor }).then((credits) => {
		empty = !credits;
		return credits;
	});
	const done = () => {
		if (empty) toast.info(t("Nothing in this project has a credit line yet."));
		else toast.success(t("Credits copied"), { description: t("Paste them into your video's description.") });
	};
	const failed = () => toast.error(t("The clipboard is not available here"));
	if (typeof ClipboardItem !== "undefined" && navigator.clipboard?.write) {
		const item = new ClipboardItem({ "text/plain": text.then((credits) => new Blob([credits], { type: "text/plain" })) });
		void navigator.clipboard.write([item]).then(done, () => void text.then((credits) => navigator.clipboard.writeText(credits)).then(done, failed));
		return;
	}
	void text.then((credits) => navigator.clipboard.writeText(credits)).then(done, failed);
}

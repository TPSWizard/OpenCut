export function downloadBlob({
	blob,
	filename,
}: {
	blob: Blob;
	filename: string;
}): void {
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = filename;
	document.body.appendChild(anchor);
	anchor.click();
	document.body.removeChild(anchor);
	URL.revokeObjectURL(url);
}

export function findScrollParent({
	element,
}: {
	element: HTMLElement;
}): HTMLElement | null {
	let parent = element.parentElement;
	while (parent) {
		const { overflow, overflowX } = window.getComputedStyle(parent);
		if (/auto|scroll/.test(overflow + overflowX)) return parent;
		parent = parent.parentElement;
	}
	return null;
}

export function isTypableDOMElement({
	element,
}: {
	element: HTMLElement;
}): boolean {
	if (element.isContentEditable) return true;

	if (element.tagName === "INPUT") {
		return !(element as HTMLInputElement).disabled;
	}

	if (element.tagName === "TEXTAREA") {
		return !(element as HTMLTextAreaElement).disabled;
	}

	return false;
}

/**
 * Asks for CORS access when an image comes from another origin, so the GPU
 * may read it (a "tainted" image makes `copyExternalImageToTexture` fail
 * inside the WASM compositor). Blob and data URLs need nothing.
 */
export function prepareImageForGpu({ image, url }: { image: HTMLImageElement; url: string }): void {
	if (/^https?:/i.test(url)) image.crossOrigin = "anonymous";
}

/** Whether the GPU may read this loaded image (false when it is tainted). */
export function isImageReadable({ image }: { image: HTMLImageElement }): boolean {
	try {
		const canvas = document.createElement("canvas");
		canvas.width = 1;
		canvas.height = 1;
		const context = canvas.getContext("2d");
		if (!context) return true;
		context.drawImage(image, 0, 0, 1, 1);
		context.getImageData(0, 0, 1, 1);
		return true;
	} catch {
		return false;
	}
}

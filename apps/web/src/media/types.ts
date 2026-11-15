import type { MediaAssetData } from "@/services/storage/types";

/** `lottie`: a Lottie animation (`.json` or `.lottie`), placed as an image element; see `@/lottie`. */
export type MediaType = "image" | "video" | "audio" | "lottie";

export interface MediaAsset
	extends Omit<MediaAssetData, "size" | "lastModified"> {
	file: File;
	url?: string;
	/** The small copy the preview decodes; export reads `file`. */
	proxyFile?: File;
}

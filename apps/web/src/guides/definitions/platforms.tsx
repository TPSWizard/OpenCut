import Image from "@/compat/image";
import type { GuideDefinition } from "@/guides/types";
import { TikTokLayout } from "./tiktok-layout";
import { getPlatformPreset, type PlatformPresetId } from "@/platforms";
import { SafeAreaOverlay } from "@/platforms/components/safe-area-overlay";

function PlatformLogo({
	domain,
	className = "size-4",
}: {
	domain: string;
	className?: string;
}) {
	return (
		<Image
			src={`https://cdn.brandfetch.io/${domain}/w/64/h/64`}
			alt=""
			width={18}
			height={18}
			className={className}
			draggable={false}
			unoptimized
		/>
	);
}

function PlatformGuidePreview({ domain }: { domain: string }) {
	return <PlatformLogo domain={domain} />;
}

/**
 * The platform's safe area from the preset catalogue, shaded over the
 * frame. Platforms without a published safe area draw nothing.
 */
function renderSafeArea({
	preset,
	width,
	height,
}: {
	preset: PlatformPresetId;
	width: number;
	height: number;
}) {
	const platform = getPlatformPreset({ id: preset });
	if (!platform?.safeArea) return null;
	return (
		<SafeAreaOverlay
			width={width}
			height={height}
			safeArea={platform.safeArea}
			label={platform.label}
		/>
	);
}

function platformGuide({
	id,
	label,
	domain,
	preset,
}: {
	id: string;
	label: string;
	domain: string;
	/** Preset whose safe area the guide shades, when it has one. */
	preset?: PlatformPresetId;
}): GuideDefinition {
	return {
		id,
		label,
		renderPreview: () => <PlatformGuidePreview domain={domain} />,
		renderTriggerIcon: () => <PlatformLogo domain={domain} />,
		renderOverlay: ({ width, height }) =>
			preset ? renderSafeArea({ preset, width, height }) : null,
	};
}

export const tiktokGuide: GuideDefinition = {
	...platformGuide({
		id: "tiktok",
		label: "TikTok",
		domain: "tiktok.com",
		preset: "tiktok",
	}),
	renderOverlay: ({ width, height }) => (
		<>
			{renderSafeArea({ preset: "tiktok", width, height })}
			<TikTokLayout />
		</>
	),
};
export const igReelsGuide = platformGuide({
	id: "ig-reels",
	label: "Reels",
	domain: "instagram.com",
	preset: "instagram-reels",
});
export const ytShortsGuide = platformGuide({
	id: "yt-shorts",
	label: "Shorts",
	domain: "youtube.com",
	preset: "youtube-shorts",
});
export const spotlightGuide = platformGuide({
	id: "spotlight",
	label: "Spotlight",
	domain: "snapchat.com",
});

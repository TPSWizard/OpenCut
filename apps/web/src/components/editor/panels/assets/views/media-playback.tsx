"use client";

import { PauseIcon, PlayIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useT } from "@/i18n";
import type { MediaAsset } from "@/media/types";
import { cn } from "@/utils/ui";

/**
 * Plays a video or audio file in the media panel before it goes on the
 * timeline: hovering a video scrubs through it (the pointer's position
 * across the card is the time), the play button plays it with sound, and
 * audio gets a play button and a progress line. One preview plays at a
 * time; leaving the card stops a video. A video's proxy plays when it has
 * one, as the editor's preview does.
 */

/** The time a hover at `x` across a card `width` wide points at, inside the media. */
export function scrubTime({ x, width, duration }: { x: number; width: number; duration: number }): number {
	if (!(width > 0) || !(duration > 0)) return 0;
	const fraction = Math.min(1, Math.max(0, x / width));
	// Stop just short of the end, where some browsers show a black frame.
	return Math.min(fraction * duration, Math.max(0, duration - 0.05));
}

let stopPlaying: (() => void) | null = null;

/** Makes `stop` the one playing preview, stopping the one before it. */
export function claimPlayback(stop: () => void): void {
	if (stopPlaying && stopPlaying !== stop) stopPlaying();
	stopPlaying = stop;
}

export function releasePlayback(stop: () => void): void {
	if (stopPlaying === stop) stopPlaying = null;
}

/** An object URL for the file to preview (the proxy when there is one), revoked when it changes. */
function usePreviewUrl({ item, active }: { item: MediaAsset; active: boolean }): string | null {
	const [url, setUrl] = useState<string | null>(null);
	const file = item.proxyFile ?? item.file;
	useEffect(() => {
		if (!active) return;
		if (!file) {
			setUrl(item.url ?? null);
			return;
		}
		const objectUrl = URL.createObjectURL(file);
		setUrl(objectUrl);
		return () => {
			URL.revokeObjectURL(objectUrl);
			setUrl(null);
		};
	}, [active, file, item.url]);
	return url;
}

/** Keeps a click on a preview control from selecting or dragging the card. */
const swallow = {
	onPointerDown: (event: React.PointerEvent) => event.stopPropagation(),
	onMouseDown: (event: React.MouseEvent) => event.stopPropagation(),
	onDoubleClick: (event: React.MouseEvent) => event.stopPropagation(),
	draggable: false,
};

function PlayButton({ playing, onToggle, className }: { playing: boolean; onToggle: () => void; className?: string }) {
	const t = useT();
	return (
		<button
			type="button"
			{...swallow}
			onClick={(event) => {
				event.stopPropagation();
				onToggle();
			}}
			aria-label={playing ? t("Pause preview") : t("Play preview")}
			title={playing ? t("Pause preview") : t("Play preview")}
			data-testid="media-preview-play"
			className={cn(
				"flex size-6 items-center justify-center rounded-full bg-black/70 text-white transition-opacity hover:bg-black/90",
				className,
			)}
		>
			<HugeiconsIcon icon={playing ? PauseIcon : PlayIcon} className="size-3.5" />
		</button>
	);
}

export function VideoPlaybackPreview({ item, scrub, children }: { item: MediaAsset; scrub: boolean; children: ReactNode }) {
	const [hovering, setHovering] = useState(false);
	const [playing, setPlaying] = useState(false);
	const [progress, setProgress] = useState(0);
	const videoRef = useRef<HTMLVideoElement | null>(null);
	const active = hovering || playing;
	const url = usePreviewUrl({ item, active });
	const stopRef = useRef<() => void>(() => {});
	stopRef.current = () => {
		videoRef.current?.pause();
		setPlaying(false);
	};
	const stop = useRef(() => stopRef.current()).current;

	useEffect(() => () => releasePlayback(stop), [stop]);

	const toggle = () => {
		const video = videoRef.current;
		if (!video) return;
		if (playing) {
			stop();
			releasePlayback(stop);
			return;
		}
		claimPlayback(stop);
		video.muted = false;
		void video.play().then(
			() => setPlaying(true),
			() => setPlaying(false),
		);
	};

	return (
		<div
			className="group relative size-full"
			data-testid="media-video-preview"
			onPointerEnter={() => setHovering(true)}
			onPointerLeave={() => {
				setHovering(false);
				if (playing) {
					stop();
					releasePlayback(stop);
				}
			}}
			onPointerMove={(event) => {
				const video = videoRef.current;
				if (!scrub || playing || !video || !(video.duration > 0)) return;
				const box = event.currentTarget.getBoundingClientRect();
				const time = scrubTime({ x: event.clientX - box.left, width: box.width, duration: video.duration });
				video.currentTime = time;
				setProgress(time / video.duration);
			}}
		>
			{children}
			{active && url ? (
				<video
					ref={videoRef}
					src={url}
					muted
					playsInline
					preload="auto"
					className="pointer-events-none absolute inset-0 size-full rounded object-cover"
					onTimeUpdate={(event) => {
						const video = event.currentTarget;
						if (video.duration > 0) setProgress(video.currentTime / video.duration);
					}}
					onEnded={() => {
						setPlaying(false);
						releasePlayback(stop);
					}}
				/>
			) : null}
			{active ? (
				<>
					<div className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-white/20">
						<div className="h-full bg-white" style={{ width: `${progress * 100}%` }} />
					</div>
					<PlayButton playing={playing} onToggle={toggle} className="absolute top-1 left-1" />
				</>
			) : null}
		</div>
	);
}

export function AudioPlaybackPreview({ item, children }: { item: MediaAsset; children: ReactNode }) {
	const [playing, setPlaying] = useState(false);
	const [progress, setProgress] = useState(0);
	const [armed, setArmed] = useState(false);
	const audioRef = useRef<HTMLAudioElement | null>(null);
	const url = usePreviewUrl({ item, active: armed });
	const stopRef = useRef<() => void>(() => {});
	stopRef.current = () => {
		audioRef.current?.pause();
		setPlaying(false);
	};
	const stop = useRef(() => stopRef.current()).current;

	useEffect(() => () => releasePlayback(stop), [stop]);

	useEffect(() => {
		const audio = audioRef.current;
		if (!armed || !url || !audio || playing) return;
		// First press: play once the element has its source.
		claimPlayback(stop);
		void audio.play().then(
			() => setPlaying(true),
			() => setPlaying(false),
		);
		// biome-ignore lint/correctness/useExhaustiveDependencies: start only when the source arrives
	}, [url]);

	const toggle = () => {
		const audio = audioRef.current;
		if (!armed || !audio) {
			setArmed(true);
			return;
		}
		if (playing) {
			stop();
			releasePlayback(stop);
			return;
		}
		claimPlayback(stop);
		void audio.play().then(
			() => setPlaying(true),
			() => setPlaying(false),
		);
	};

	return (
		<div className="group relative size-full" data-testid="media-audio-preview">
			{children}
			{armed && url ? (
				// biome-ignore lint/a11y/useMediaCaption: a preview of the user's own audio file
				<audio
					ref={audioRef}
					src={url}
					preload="auto"
					onTimeUpdate={(event) => {
						const audio = event.currentTarget;
						if (audio.duration > 0) setProgress(audio.currentTime / audio.duration);
					}}
					onEnded={() => {
						setPlaying(false);
						setProgress(0);
						releasePlayback(stop);
					}}
				/>
			) : null}
			<PlayButton
				playing={playing}
				onToggle={toggle}
				className={cn("absolute top-1 left-1", !playing && "opacity-0 group-hover:opacity-100 focus-visible:opacity-100")}
			/>
			{progress > 0 ? (
				<div className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-black/10">
					<div className="bg-primary h-full" style={{ width: `${progress * 100}%` }} />
				</div>
			) : null}
		</div>
	);
}

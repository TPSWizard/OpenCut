"use client";

import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { formatMeterLufs, meterFraction, type MeterReadings } from "@/loudness/mix";
import { canTrackHaveAudio } from "@/timeline/track-capabilities";
import { getTrackMixSettings, TRACK_VOLUME_MAX_DB, TRACK_VOLUME_MIN_DB } from "@/timeline/track-mix";
import type { TimelineTrack } from "@/timeline/types";
import { cn } from "@/utils/ui";

/**
 * The track mixer: a strip per track that carries sound, with mute, solo,
 * volume (dB), pan and a meter while playing (`@/timeline/track-mix`, the
 * shared crate the desktop uses).
 */
export function MixerPanel() {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor((e) => e.scenes.getActiveSceneOrNull()?.tracks);
	const isPlaying = useEditor((e) => e.playback.getIsPlaying());
	const [meters, setMeters] = useState<Map<string, MeterReadings>>(new Map());

	useEffect(() => {
		if (!isPlaying) {
			setMeters(new Map());
			return;
		}
		let frame = 0;
		let last = 0;
		const tick = (now: number) => {
			if (now - last > 100) {
				last = now;
				setMeters(editor.audio.getTrackMeterReadings());
			}
			frame = requestAnimationFrame(tick);
		};
		frame = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(frame);
	}, [editor, isPlaying]);

	if (!tracks) return null;
	const sounding = [...tracks.overlay, tracks.main, ...tracks.audio].filter((track) => canTrackHaveAudio(track));
	if (sounding.length === 0) return null;

	return (
		<div className="flex w-full flex-col gap-2" data-testid="mixer-panel">
			<div className="flex flex-col gap-0.5">
				<p className="text-sm font-medium">{t("Mixer")}</p>
				<p className="text-muted-foreground text-xs">
					{t("Each track's volume (dB, -60 is silent) and pan, after each clip's own. Solo plays only the soloed tracks; mute wins.")}
				</p>
			</div>
			{sounding.map((track) => (
				<MixerStrip key={track.id} track={track} reading={meters.get(track.id)} />
			))}
		</div>
	);
}

function MixerStrip({ track, reading }: { track: TimelineTrack; reading?: MeterReadings }) {
	const t = useT();
	const editor = useEditor();
	const mix = getTrackMixSettings(track);
	const [volume, setVolume] = useState(String(mix.volume));
	const [pan, setPan] = useState(String(mix.pan));
	useEffect(() => setVolume(String(mix.volume)), [mix.volume]);
	useEffect(() => setPan(String(mix.pan)), [mix.pan]);
	const commit = (key: "volume" | "pan", text: string) => {
		const value = Number(text);
		if (!Number.isFinite(value) || value === mix[key]) return;
		editor.timeline.setTrackMix({ trackId: track.id, [key]: value });
	};
	const peak = reading?.peakDbfs ?? Number.NEGATIVE_INFINITY;
	const colour = peak >= -1 ? "bg-red-500" : peak >= -6 ? "bg-yellow-500" : "bg-green-500";

	return (
		<div className="flex flex-col gap-1.5 rounded-sm border p-2" data-testid={`mixer-strip-${track.id}`}>
			<div className="flex items-center gap-1.5">
				<span className="min-w-0 flex-1 truncate text-xs">{track.name}</span>
				<Button
					size="sm"
					variant={mix.muted ? "default" : "outline"}
					className="h-6 w-6 p-0 text-xs"
					title={t("Mute")}
					data-testid="mixer-mute"
					onClick={() => editor.timeline.toggleTrackMute({ trackId: track.id })}
				>
					M
				</Button>
				<Button
					size="sm"
					variant={mix.solo ? "default" : "outline"}
					className={cn("h-6 w-6 p-0 text-xs", mix.solo && "bg-yellow-500 text-black hover:bg-yellow-400")}
					title={t("Solo")}
					data-testid="mixer-solo"
					onClick={() => editor.timeline.setTrackMix({ trackId: track.id, solo: !mix.solo })}
				>
					S
				</Button>
			</div>
			<div className="flex items-center gap-1.5">
				<span className="text-muted-foreground text-xs">dB</span>
				<Input
					className="h-7 min-w-0 flex-1 text-xs"
					type="number"
					step={0.5}
					min={TRACK_VOLUME_MIN_DB}
					max={TRACK_VOLUME_MAX_DB}
					value={volume}
					data-testid="mixer-volume"
					onFocus={(event) => event.target.select()}
					onChange={(event) => setVolume(event.target.value)}
					onBlur={() => commit("volume", volume)}
					onKeyDown={(event) => event.key === "Enter" && commit("volume", volume)}
				/>
				<span className="text-muted-foreground text-xs">{t("Pan")}</span>
				<Input
					className="h-7 min-w-0 flex-1 text-xs"
					type="number"
					step={0.1}
					min={-1}
					max={1}
					value={pan}
					data-testid="mixer-pan"
					onFocus={(event) => event.target.select()}
					onChange={(event) => setPan(event.target.value)}
					onBlur={() => commit("pan", pan)}
					onKeyDown={(event) => event.key === "Enter" && commit("pan", pan)}
				/>
			</div>
			<div className="flex items-center gap-1.5">
				<div className="bg-muted h-1.5 min-w-0 flex-1 overflow-hidden rounded-full">
					<div className={cn("h-full rounded-full", colour)} style={{ width: `${meterFraction(peak) * 100}%` }} />
				</div>
				<span className="text-muted-foreground w-9 text-right font-mono text-[10px]" data-testid="mixer-lufs">
					{reading ? formatMeterLufs(reading.momentaryLufs) : "–"}
				</span>
			</div>
		</div>
	);
}

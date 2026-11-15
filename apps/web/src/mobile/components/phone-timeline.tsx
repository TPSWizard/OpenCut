"use client";

import {
	type PointerEvent as ReactPointerEvent,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { toast } from "sonner";
import { useEditor } from "@/editor/use-editor";
import { useT } from "@/i18n";
import { cn } from "@/utils/ui";
import type {
	ElementRef,
	SceneTracks,
	TimelineElement,
	TimelineTrack,
} from "@/timeline";
import { getDisplayTracks } from "@/timeline/group-move/track-placement";
import { buildResizeMembers } from "@/timeline/controllers/resize-controller";
import {
	computeGroupResize,
	type GroupResizeMember,
	type GroupResizeResult,
	type ResizeSide,
} from "@/timeline/group-resize";
import {
	type MediaTime,
	mediaTime,
	mediaTimeFromSeconds,
	mediaTimeToSeconds,
	TICKS_PER_SECOND,
} from "@/wasm";
import type { MediaAsset } from "@/media/types";
import {
	clampPps,
	formatClock,
	PHONE_DEFAULT_PPS,
	rowAt,
	rulerStep,
	scrollForTime,
	timeForScroll,
	zoomByPinch,
} from "../timeline-math";
import { clipEdges, planPhoneMove, snapStart } from "../move";

const ROW_HEIGHT = 52;
const ROW_GAP = 6;
const RULER_HEIGHT = 22;
/** How close (in pixels) an edge has to come to another to snap to it. */
const SNAP_PIXELS = 10;
/** A press that moves less than this is a tap, not a drag. */
const TAP_PIXELS = 6;

type Gesture =
	| {
			kind: "move";
			pointerId: number;
			ref: ElementRef;
			element: TimelineElement;
			startX: number;
			startY: number;
			dx: number;
			dy: number;
			moved: boolean;
	  }
	| {
			kind: "trim";
			pointerId: number;
			side: ResizeSide;
			startX: number;
			members: GroupResizeMember[];
			result: GroupResizeResult | null;
	  };

/** Keeps the drag's pointer events coming to this element, where the browser allows it. */
function capturePointer(event: ReactPointerEvent) {
	try {
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	} catch {
		// A pointer the browser no longer tracks; the drag still works while over the timeline.
	}
}

function ticksForPixels({ pixels, pps }: { pixels: number; pps: number }): number {
	return Math.round((pixels / pps) * TICKS_PER_SECOND);
}

const CLIP_COLORS: Record<string, string> = {
	video: "bg-sky-600/80 border-sky-400",
	image: "bg-sky-600/80 border-sky-400",
	audio: "bg-emerald-600/80 border-emerald-400",
	text: "bg-amber-600/85 border-amber-400",
	sticker: "bg-pink-600/80 border-pink-400",
	graphic: "bg-pink-600/80 border-pink-400",
	effect: "bg-violet-600/80 border-violet-400",
	compound: "bg-indigo-600/80 border-indigo-400",
};

function clipColor(type: string): string {
	return CLIP_COLORS[type] ?? "bg-slate-600/80 border-slate-400";
}

function clipBackground({
	element,
	assets,
}: {
	element: TimelineElement;
	assets: Map<string, MediaAsset>;
}): string | undefined {
	if (!("mediaId" in element) || typeof element.mediaId !== "string") {
		return undefined;
	}
	const asset = assets.get(element.mediaId);
	const url =
		asset?.thumbnailUrl ?? (asset?.type === "image" ? asset.url : undefined);
	return url ? `url("${url}")` : undefined;
}

/**
 * The phone timeline. The playhead stays in the middle and the clips scroll
 * under it: swiping seeks, pinching zooms, tapping a clip selects it,
 * dragging a selected clip moves it (to another track too), and the handles
 * on a selected clip trim it.
 */
export function PhoneTimeline() {
	const t = useT();
	const editor = useEditor();
	const tracks = useEditor(
		(e) =>
			e.timeline.getPreviewTracks() ??
			e.scenes.getActiveSceneOrNull()?.tracks ??
			null,
	);
	const selected = useEditor((e) => e.selection.getSelectedElements());
	const totalTicks = useEditor((e) => e.timeline.getTotalDuration());
	const assetList = useEditor((e) => e.media.getAssets());
	const assets = useMemo(
		() => new Map(assetList.map((asset) => [asset.id, asset])),
		[assetList],
	);
	const duration = mediaTimeToSeconds({ time: totalTicks });

	const [pps, setPps] = useState(PHONE_DEFAULT_PPS);
	const ppsRef = useRef(pps);
	ppsRef.current = pps;
	const durationRef = useRef(duration);
	durationRef.current = duration;

	const scrollRef = useRef<HTMLDivElement>(null);
	const rowsRef = useRef<HTMLDivElement>(null);
	const [viewport, setViewport] = useState(0);
	const expectedScroll = useRef(0);
	const userScrolling = useRef(false);
	const scrollIdle = useRef<ReturnType<typeof setTimeout> | null>(null);
	const [gesture, setGesture] = useState<Gesture | null>(null);
	const gestureRef = useRef<Gesture | null>(null);
	gestureRef.current = gesture;

	const rows: TimelineTrack[] = useMemo(
		() => (tracks ? getDisplayTracks({ tracks }) : []),
		[tracks],
	);
	const selectedIds = useMemo(
		() => new Set(selected.map((ref) => ref.elementId)),
		[selected],
	);

	useLayoutEffect(() => {
		const element = scrollRef.current;
		if (!element) return;
		const observer = new ResizeObserver(() => setViewport(element.clientWidth));
		observer.observe(element);
		setViewport(element.clientWidth);
		return () => observer.disconnect();
	}, []);

	const scrollToTime = useCallback((time: MediaTime) => {
		const element = scrollRef.current;
		if (!element) return;
		const left = scrollForTime({
			seconds: mediaTimeToSeconds({ time }),
			pps: ppsRef.current,
		});
		expectedScroll.current = left;
		element.scrollLeft = left;
		// The browser may clamp it; remember what it really is.
		expectedScroll.current = element.scrollLeft;
	}, []);

	// The clips follow playback and seeks made elsewhere (the preview's
	// buttons, the properties panel), unless a finger is on the timeline.
	useEffect(() => {
		const follow = (time: MediaTime) => {
			if (userScrolling.current) return;
			scrollToTime(time);
		};
		const offUpdate = editor.playback.onUpdate(follow);
		const offSeek = editor.playback.onSeek(follow);
		return () => {
			offUpdate();
			offSeek();
		};
	}, [editor, scrollToTime]);

	// A zoom or a new viewport keeps the playhead's time in the middle.
	useLayoutEffect(() => {
		scrollToTime(editor.playback.getCurrentTime());
	}, [pps, viewport, scrollToTime, editor]);

	const markUserScroll = useCallback(() => {
		userScrolling.current = true;
		if (scrollIdle.current) clearTimeout(scrollIdle.current);
		scrollIdle.current = setTimeout(() => {
			userScrolling.current = false;
		}, 220);
	}, []);

	const handleScroll = () => {
		const element = scrollRef.current;
		if (!element) return;
		if (Math.abs(element.scrollLeft - expectedScroll.current) < 1) return;
		expectedScroll.current = element.scrollLeft;
		markUserScroll();
		if (editor.playback.getIsPlaying()) editor.playback.pause();
		const seconds = timeForScroll({
			scrollLeft: element.scrollLeft,
			pps: ppsRef.current,
			duration: durationRef.current,
		});
		editor.playback.seek({ time: mediaTimeFromSeconds({ seconds }) });
	};

	// Pinch to zoom (two fingers), and ctrl + wheel on a trackpad or mouse.
	useEffect(() => {
		const element = scrollRef.current;
		if (!element) return;
		let pinch: { distance: number; pps: number } | null = null;
		const distanceOf = (touches: TouchList) =>
			Math.hypot(
				touches[0].clientX - touches[1].clientX,
				touches[0].clientY - touches[1].clientY,
			);
		const onTouchStart = (event: TouchEvent) => {
			markUserScroll();
			if (event.touches.length === 2) {
				pinch = { distance: distanceOf(event.touches), pps: ppsRef.current };
			}
		};
		const onTouchMove = (event: TouchEvent) => {
			markUserScroll();
			if (pinch && event.touches.length === 2) {
				event.preventDefault();
				setPps(
					zoomByPinch({
						pps: pinch.pps,
						startDistance: pinch.distance,
						distance: distanceOf(event.touches),
					}),
				);
			}
		};
		const onTouchEnd = (event: TouchEvent) => {
			if (event.touches.length < 2) pinch = null;
			markUserScroll();
		};
		const onWheel = (event: WheelEvent) => {
			if (event.ctrlKey || event.metaKey) {
				event.preventDefault();
				setPps((current) => clampPps(current * Math.exp(-event.deltaY / 200)));
				return;
			}
			markUserScroll();
		};
		element.addEventListener("touchstart", onTouchStart, { passive: true });
		element.addEventListener("touchmove", onTouchMove, { passive: false });
		element.addEventListener("touchend", onTouchEnd, { passive: true });
		element.addEventListener("touchcancel", onTouchEnd, { passive: true });
		element.addEventListener("wheel", onWheel, { passive: false });
		return () => {
			element.removeEventListener("touchstart", onTouchStart);
			element.removeEventListener("touchmove", onTouchMove);
			element.removeEventListener("touchend", onTouchEnd);
			element.removeEventListener("touchcancel", onTouchEnd);
			element.removeEventListener("wheel", onWheel);
		};
	}, [markUserScroll]);

	const half = viewport / 2;
	const contentWidth = Math.max(viewport, viewport + duration * pps);

	const select = (ref: ElementRef | null) => {
		editor.selection.setSelectedElements({ elements: ref ? [ref] : [] });
	};

	const sceneTracks = (): SceneTracks | null =>
		editor.scenes.getActiveSceneOrNull()?.tracks ?? null;

	// --- Move a selected clip ---

	const startMove = (
		event: ReactPointerEvent,
		track: TimelineTrack,
		element: TimelineElement,
	) => {
		if (track.locked) return;
		event.stopPropagation();
		capturePointer(event);
		setGesture({
			kind: "move",
			pointerId: event.pointerId,
			ref: { trackId: track.id, elementId: element.id },
			element,
			startX: event.clientX,
			startY: event.clientY,
			dx: 0,
			dy: 0,
			moved: false,
		});
	};

	const moveTarget = (current: Extract<Gesture, { kind: "move" }>) => {
		const tracksNow = sceneTracks();
		if (!tracksNow) return null;
		const rawStart =
			current.element.startTime +
			ticksForPixels({ pixels: current.dx, pps: ppsRef.current });
		const start = snapStart({
			start: Math.max(0, rawStart),
			duration: current.element.duration,
			edges: [
				...clipEdges({ tracks: tracksNow, exclude: selectedIds }),
				editor.playback.getCurrentTime(),
			],
			threshold: ticksForPixels({ pixels: SNAP_PIXELS, pps: ppsRef.current }),
		});
		const display = getDisplayTracks({ tracks: tracksNow });
		const fromIndex = display.findIndex((row) => row.id === current.ref.trackId);
		const rowsTop = rowsRef.current?.getBoundingClientRect().top ?? 0;
		const pointerRow = rowAt({
			y: current.startY + current.dy - rowsTop,
			rowHeight: ROW_HEIGHT + ROW_GAP,
			rowCount: display.length,
		});
		const target = display[pointerRow ?? fromIndex] ?? display[fromIndex];
		return { tracks: tracksNow, start, target, fromIndex, display };
	};

	// --- Trim a selected clip ---

	const startTrim = (
		event: ReactPointerEvent,
		track: TimelineTrack,
		element: TimelineElement,
		side: ResizeSide,
	) => {
		if (track.locked) return;
		event.stopPropagation();
		const tracksNow = sceneTracks();
		if (!tracksNow) return;
		const members = buildResizeMembers({
			tracks: tracksNow,
			selectedElements: [{ trackId: track.id, elementId: element.id }],
		});
		if (members.length === 0) return;
		capturePointer(event);
		editor.timeline.discardPreview();
		setGesture({
			kind: "trim",
			pointerId: event.pointerId,
			side,
			startX: event.clientX,
			members,
			result: null,
		});
	};

	const onPointerMove = (event: ReactPointerEvent) => {
		const current = gestureRef.current;
		if (!current || event.pointerId !== current.pointerId) return;
		if (current.kind === "move") {
			const dx = event.clientX - current.startX;
			const dy = event.clientY - current.startY;
			setGesture({
				...current,
				dx,
				dy,
				moved: current.moved || Math.hypot(dx, dy) > TAP_PIXELS,
			});
			return;
		}
		const fps = editor.project.getActive()?.settings.fps;
		const tracksNow = sceneTracks();
		if (!fps || !tracksNow) return;
		const member = current.members[0];
		const edge =
			current.side === "left"
				? member.startTime
				: member.startTime + member.duration;
		const raw =
			edge +
			ticksForPixels({ pixels: event.clientX - current.startX, pps: ppsRef.current });
		const snapped = snapStart({
			start: raw,
			duration: 0,
			edges: [
				...clipEdges({
					tracks: tracksNow,
					exclude: new Set(current.members.map((m) => m.elementId)),
				}),
				editor.playback.getCurrentTime(),
			],
			threshold: ticksForPixels({ pixels: SNAP_PIXELS, pps: ppsRef.current }),
		});
		const result = computeGroupResize({
			members: current.members,
			side: current.side,
			deltaTime: mediaTime({ ticks: snapped - edge }),
			fps,
		});
		editor.timeline.previewElements({
			updates: result.updates.map(({ trackId, elementId, patch }) => ({
				trackId,
				elementId,
				updates: patch as Partial<TimelineElement>,
			})),
		});
		setGesture({ ...current, result });
	};

	const onPointerUp = (event: ReactPointerEvent) => {
		const current = gestureRef.current;
		if (!current || event.pointerId !== current.pointerId) return;
		setGesture(null);
		if (current.kind === "trim") {
			editor.timeline.discardPreview();
			const updates = current.result?.updates ?? [];
			const changed = updates.some((update) => {
				const member = current.members.find(
					(m) => m.elementId === update.elementId,
				);
				return (
					member?.startTime !== update.patch.startTime ||
					member?.duration !== update.patch.duration ||
					member?.trimStart !== update.patch.trimStart ||
					member?.trimEnd !== update.patch.trimEnd
				);
			});
			if (changed) {
				editor.timeline.updateElements({
					updates: updates.map(({ trackId, elementId, patch }) => ({
						trackId,
						elementId,
						patch: patch as Partial<TimelineElement>,
					})),
				});
			}
			return;
		}
		if (!current.moved) return;
		const target = moveTarget(current);
		if (!target?.target) return;
		const plan = planPhoneMove({
			tracks: target.tracks,
			ref: current.ref,
			selected,
			startTime: mediaTime({ ticks: target.start }),
			targetTrackId: target.target.id,
		});
		if (!plan) {
			toast.error(t("That clip can't go there"));
			return;
		}
		editor.timeline.moveElements({
			moves: plan.moves,
			createTracks: plan.createTracks,
		});
		editor.selection.setSelectedElements({ elements: plan.targetSelection });
	};

	const onPointerCancel = (event: ReactPointerEvent) => {
		const current = gestureRef.current;
		if (!current || event.pointerId !== current.pointerId) return;
		if (current.kind === "trim") editor.timeline.discardPreview();
		setGesture(null);
	};

	const step = rulerStep({ pps });
	const ticks: number[] = [];
	for (let time = 0; time <= duration + step; time += step) ticks.push(time);

	const moving = gesture?.kind === "move" && gesture.moved ? gesture : null;
	const moveInfo = moving ? moveTarget(moving) : null;

	if (!tracks) return null;

	const empty = rows.every((row) => row.elements.length === 0);

	return (
		<div className="relative flex min-h-0 flex-1 flex-col" data-testid="phone-timeline">
			<div
				ref={scrollRef}
				className="relative min-h-0 flex-1 overflow-x-auto overflow-y-auto overscroll-contain [scrollbar-width:none]"
				onScroll={handleScroll}
				onClick={() => select(null)}
				onPointerMove={onPointerMove}
				onPointerUp={onPointerUp}
				onPointerCancel={onPointerCancel}
			>
				<div className="relative" style={{ width: contentWidth }}>
					<div
						className="bg-background/95 sticky top-0 z-20 border-b"
						style={{ height: RULER_HEIGHT }}
					>
						{ticks.map((time) => (
							<span
								key={time}
								className="text-muted-foreground absolute top-1 -translate-x-1/2 font-mono text-[10px] tabular-nums"
								style={{ left: half + time * pps }}
							>
								{formatClock(time, { tenths: step < 1 })}
							</span>
						))}
					</div>
					<div
						ref={rowsRef}
						className="relative flex flex-col pt-2 pb-4"
						style={{ gap: ROW_GAP }}
					>
						{rows.map((track, rowIndex) => {
							const isTarget =
								moveInfo?.target?.id === track.id &&
								moveInfo.fromIndex !== rowIndex;
							return (
								<div
									key={track.id}
									className={cn(
										"relative",
										track.id === tracks.main.id && "bg-muted/30",
										isTarget && "bg-primary/15",
										track.locked && "opacity-60",
									)}
									style={{ height: ROW_HEIGHT }}
									data-testid="phone-track"
								>
									{track.elements.map((element) => {
										const isSelected = selectedIds.has(element.id);
										const isMoving =
											moving?.ref.elementId === element.id;
										const start = mediaTimeToSeconds({ time: element.startTime });
										const length = mediaTimeToSeconds({ time: element.duration });
										const background = clipBackground({ element, assets });
										const left = half + start * pps;
										const width = Math.max(4, length * pps);
										const moveLeft =
											isMoving && moveInfo
												? half +
													mediaTimeToSeconds({ time: mediaTime({ ticks: moveInfo.start }) }) * pps -
													left
												: 0;
										return (
											<div
												key={element.id}
												role="button"
												tabIndex={0}
												aria-pressed={isSelected}
												aria-label={element.name}
												data-testid="phone-clip"
												data-element-id={element.id}
												className={cn(
													"absolute top-0 bottom-0 overflow-hidden rounded-md border text-white select-none",
													clipColor(element.type),
													isSelected && "z-10 ring-2 ring-white",
													isSelected && !track.locked && "touch-none",
													isMoving && "z-30 opacity-90 shadow-xl",
												)}
												style={{
													left,
													width,
													backgroundImage: background,
													backgroundSize: background ? "auto 100%" : undefined,
													backgroundRepeat: background ? "repeat-x" : undefined,
													transform: isMoving
														? `translate(${moveLeft}px, ${moving?.dy ?? 0}px)`
														: undefined,
												}}
												onClick={(event) => {
													event.stopPropagation();
													if (!track.locked) {
														select({ trackId: track.id, elementId: element.id });
													}
												}}
												onKeyDown={(event) => {
													if (event.key === "Enter" || event.key === " ") {
														event.preventDefault();
														select({ trackId: track.id, elementId: element.id });
													}
												}}
												onPointerDown={
													isSelected
														? (event) => startMove(event, track, element)
														: undefined
												}
											>
												<span className="absolute inset-x-0 bottom-0 truncate bg-black/45 px-1.5 py-0.5 text-[10px] leading-tight">
													{element.name}
												</span>
												{isSelected && !track.locked && (
													<>
														<TrimHandle
															side="left"
															label={t("Trim start")}
															onPointerDown={(event) =>
																startTrim(event, track, element, "left")
															}
														/>
														<TrimHandle
															side="right"
															label={t("Trim end")}
															onPointerDown={(event) =>
																startTrim(event, track, element, "right")
															}
														/>
													</>
												)}
											</div>
										);
									})}
								</div>
							);
						})}
					</div>
				</div>
			</div>
			<div
				className="bg-primary pointer-events-none absolute top-0 bottom-0 left-1/2 z-30 w-0.5 -translate-x-1/2"
				data-testid="phone-playhead"
			/>
			{empty && (
				<p className="text-muted-foreground pointer-events-none absolute inset-x-0 top-1/2 px-8 text-center text-xs">
					{t("Tap Media below to add videos and photos.")}
				</p>
			)}
		</div>
	);
}

function TrimHandle({
	side,
	label,
	onPointerDown,
}: {
	side: ResizeSide;
	label: string;
	onPointerDown: (event: ReactPointerEvent) => void;
}) {
	return (
		<div
			role="slider"
			aria-label={label}
			aria-valuenow={0}
			tabIndex={-1}
			data-testid={`phone-trim-${side}`}
			className={cn(
				"absolute top-0 bottom-0 z-20 flex w-5 touch-none items-center justify-center bg-white",
				side === "left" ? "left-0 rounded-l-md" : "right-0 rounded-r-md",
			)}
			onPointerDown={onPointerDown}
			onClick={(event) => event.stopPropagation()}
		>
			<span className="h-5 w-0.5 rounded-full bg-black/60" />
		</div>
	);
}

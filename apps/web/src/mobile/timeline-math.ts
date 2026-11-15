/**
 * Pure helpers for the phone timeline: the playhead stays in the middle of
 * the screen and the clips scroll under it, so scrolling is seeking.
 */

/** Pixels per second the phone timeline starts at. */
export const PHONE_DEFAULT_PPS = 48;
export const PHONE_MIN_PPS = 4;
export const PHONE_MAX_PPS = 480;

export function clampPps(pps: number): number {
	if (!Number.isFinite(pps)) return PHONE_DEFAULT_PPS;
	return Math.min(PHONE_MAX_PPS, Math.max(PHONE_MIN_PPS, pps));
}

/** The scroll offset that puts `seconds` under the centred playhead. */
export function scrollForTime({
	seconds,
	pps,
}: {
	seconds: number;
	pps: number;
}): number {
	return Math.max(0, seconds) * pps;
}

/** The time under the centred playhead at `scrollLeft`, within the timeline. */
export function timeForScroll({
	scrollLeft,
	pps,
	duration,
}: {
	scrollLeft: number;
	pps: number;
	duration: number;
}): number {
	const seconds = scrollLeft / pps;
	return Math.min(Math.max(0, duration), Math.max(0, seconds));
}

/** The zoom after a pinch from `startDistance` to `distance` pixels apart. */
export function zoomByPinch({
	pps,
	startDistance,
	distance,
}: {
	pps: number;
	startDistance: number;
	distance: number;
}): number {
	if (startDistance <= 0 || distance <= 0) return clampPps(pps);
	return clampPps(pps * (distance / startDistance));
}

const RULER_STEPS = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600];

/** Seconds between ruler labels, so labels sit at least `minGap` pixels apart. */
export function rulerStep({
	pps,
	minGap = 64,
}: {
	pps: number;
	minGap?: number;
}): number {
	for (const step of RULER_STEPS) {
		if (step * pps >= minGap) return step;
	}
	return RULER_STEPS[RULER_STEPS.length - 1];
}

/** `m:ss`, or `h:mm:ss` from an hour; tenths when `tenths` is set. */
export function formatClock(seconds: number, { tenths = false } = {}): string {
	const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0);
	const whole = tenths ? Math.floor(safe * 10) / 10 : Math.floor(safe);
	const hours = Math.floor(whole / 3600);
	const minutes = Math.floor((whole % 3600) / 60);
	const secs = whole % 60;
	const secText = tenths
		? secs.toFixed(1).padStart(4, "0")
		: String(Math.floor(secs)).padStart(2, "0");
	return hours > 0
		? `${hours}:${String(minutes).padStart(2, "0")}:${secText}`
		: `${minutes}:${secText}`;
}

/** The row under `y` pixels from the top of the rows, or null outside them. */
export function rowAt({
	y,
	rowHeight,
	rowCount,
}: {
	y: number;
	rowHeight: number;
	rowCount: number;
}): number | null {
	if (y < 0) return null;
	const index = Math.floor(y / rowHeight);
	return index < rowCount ? index : null;
}

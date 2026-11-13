"use client";

import {
	type KeyboardEvent,
	type PointerEvent as ReactPointerEvent,
	type PropsWithChildren,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";

type HandlebarsProps = PropsWithChildren<{
	/** Names for the two handles, read by screen readers. */
	startLabel?: string;
	endLabel?: string;
}>;

type Side = "left" | "right";

const MIN_HANDLE_SEPARATION_PX = 60;
const MASK_GRADIENT_EDGE_PADDING_PX = 10;
const HANDLEBARS_ROTATE_DEG = 2.76;
const KEY_STEP_PX = 8;
const KEY_BIG_STEP_PX = 32;

const HANDLE_CLASS =
	"bg-background absolute top-0 z-20 flex h-full w-7 cursor-ew-resize touch-none items-center justify-center rounded-full border border-yellow-500 select-none outline-none focus-visible:ring-2 focus-visible:ring-yellow-500 focus-visible:ring-offset-2 focus-visible:ring-offset-background";

/**
 * The headline's trim handles: drag either one (or focus it and use the
 * arrow keys) to trim the text the way a clip is trimmed on the timeline.
 */
export function Handlebars({
	children,
	startLabel = "Trim the start of the headline",
	endLabel = "Trim the end of the headline",
}: HandlebarsProps) {
	const containerRef = useRef<HTMLSpanElement>(null);

	const [width, setWidth] = useState(0);
	const [leftHandle, setLeftHandle] = useState(0);
	/**
	 * The right handle is kept as a distance from the right edge, so it sits
	 * correctly before the width is measured and the layout never jumps.
	 */
	const [rightInset, setRightInset] = useState(0);

	const stateRef = useRef({ width: 0, leftHandle: 0, rightInset: 0 });
	const dragRef = useRef<{
		side: Side | null;
		pointerId: number | null;
		startX: number;
		initial: number;
	}>({ side: null, pointerId: null, startX: 0, initial: 0 });

	useLayoutEffect(() => {
		const container = containerRef.current;
		if (!container) return;
		const updateWidth = () => setWidth(container.offsetWidth);
		const observer = new ResizeObserver(updateWidth);
		observer.observe(container);
		updateWidth();
		return () => observer.disconnect();
	}, []);

	useEffect(() => {
		stateRef.current = { width, leftHandle, rightInset };
	}, [width, leftHandle, rightInset]);

	// These read the latest positions from a ref, so the window listeners
	// below can be attached once.
	const moveRef = useRef({
		left: (next: number) => {
			const { width: w, rightInset: inset } = stateRef.current;
			const max = Math.max(0, w - inset - MIN_HANDLE_SEPARATION_PX);
			setLeftHandle(Math.max(0, Math.min(max, next)));
		},
		right: (nextInset: number) => {
			const { width: w, leftHandle: left } = stateRef.current;
			const max = Math.max(0, w - left - MIN_HANDLE_SEPARATION_PX);
			setRightInset(Math.max(0, Math.min(max, nextInset)));
		},
	});

	useEffect(() => {
		const handlePointerMove = (event: PointerEvent) => {
			const { side, pointerId, startX, initial } = dragRef.current;
			if (!side) return;
			if (pointerId !== null && event.pointerId !== pointerId) return;
			const deltaX = event.clientX - startX;
			if (side === "left") moveRef.current.left(initial + deltaX);
			else moveRef.current.right(initial - deltaX);
		};
		const handlePointerEnd = (event: PointerEvent) => {
			const { pointerId } = dragRef.current;
			if (pointerId !== null && event.pointerId !== pointerId) return;
			dragRef.current.side = null;
			dragRef.current.pointerId = null;
		};
		window.addEventListener("pointermove", handlePointerMove);
		window.addEventListener("pointerup", handlePointerEnd);
		window.addEventListener("pointercancel", handlePointerEnd);
		return () => {
			window.removeEventListener("pointermove", handlePointerMove);
			window.removeEventListener("pointerup", handlePointerEnd);
			window.removeEventListener("pointercancel", handlePointerEnd);
		};
	}, []);

	const startDrag =
		(side: Side) => (event: ReactPointerEvent<HTMLSpanElement>) => {
			event.preventDefault();
			event.currentTarget.setPointerCapture(event.pointerId);
			event.currentTarget.focus({ preventScroll: true });
			dragRef.current = {
				side,
				pointerId: event.pointerId,
				startX: event.clientX,
				initial:
					side === "left"
						? stateRef.current.leftHandle
						: stateRef.current.rightInset,
			};
		};

	const handleKey =
		(side: Side) => (event: KeyboardEvent<HTMLSpanElement>) => {
			const step = event.shiftKey ? KEY_BIG_STEP_PX : KEY_STEP_PX;
			// A positive delta moves the handle to the right on screen.
			let delta = 0;
			if (event.key === "ArrowRight" || event.key === "ArrowUp") delta = step;
			else if (event.key === "ArrowLeft" || event.key === "ArrowDown")
				delta = -step;
			else if (event.key === "Home") delta = -100_000;
			else if (event.key === "End") delta = 100_000;
			else return;
			event.preventDefault();
			const { leftHandle: left, rightInset: inset } = stateRef.current;
			if (side === "left") moveRef.current.left(left + delta);
			else moveRef.current.right(inset - delta);
		};

	const hasMeasuredWidth = width > 0;
	const rightHandle = width - rightInset;
	const isTrimmed = leftHandle > 0 || rightInset > 0;
	const leftGradientPercent = hasMeasuredWidth
		? (leftHandle / (width - MASK_GRADIENT_EDGE_PADDING_PX)) * 100
		: 0;
	const rightGradientPercent = hasMeasuredWidth
		? (rightHandle / (width + MASK_GRADIENT_EDGE_PADDING_PX)) * 100
		: 100;
	const textMask =
		hasMeasuredWidth && isTrimmed
			? `linear-gradient(90deg,
            rgba(255, 255, 255, 0) 0%,
            rgba(255, 255, 255, 0) ${leftGradientPercent}%,
            rgba(0, 0, 0) ${leftGradientPercent}%,
            rgba(0, 0, 0) ${rightGradientPercent}%,
            rgba(255, 255, 255, 0) ${rightGradientPercent}%,
            rgba(255, 255, 255, 0) 100%)`
			: undefined;

	const percent = (value: number) =>
		hasMeasuredWidth ? Math.round((value / width) * 100) : 0;

	return (
		<span className="flex justify-center gap-4 leading-16">
			<span
				ref={containerRef}
				className="relative mt-0.5 block"
				style={{ transform: `rotate(-${HANDLEBARS_ROTATE_DEG}deg)` }}
			>
				<span className="absolute inset-0 z-10 block size-full rounded-2xl border border-yellow-500">
					<span
						role="slider"
						tabIndex={0}
						aria-label={startLabel}
						aria-orientation="horizontal"
						aria-valuemin={0}
						aria-valuemax={100}
						aria-valuenow={percent(leftHandle)}
						className={`${HANDLE_CLASS} left-0`}
						style={{ translate: `${leftHandle}px 0` }}
						onPointerDown={startDrag("left")}
						onKeyDown={handleKey("left")}
					>
						<span className="block h-8 w-2 rounded-full bg-yellow-500" />
					</span>

					<span
						role="slider"
						tabIndex={0}
						aria-label={endLabel}
						aria-orientation="horizontal"
						aria-valuemin={0}
						aria-valuemax={100}
						aria-valuenow={hasMeasuredWidth ? percent(rightHandle) : 100}
						className={`${HANDLE_CLASS} right-0`}
						style={{ translate: `${-rightInset}px 0` }}
						onPointerDown={startDrag("right")}
						onKeyDown={handleKey("right")}
					>
						<span className="block h-8 w-2 rounded-full bg-yellow-500" />
					</span>
				</span>

				<span
					aria-hidden="true"
					className="relative z-0 inline-flex size-full items-center justify-center rounded-2xl px-9"
					style={{ mask: textMask, WebkitMask: textMask }}
				>
					{children}
				</span>
			</span>
		</span>
	);
}

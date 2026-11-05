import type { SafeArea } from "@/platforms/catalogue";

/**
 * Shades the part of the frame a platform's own controls cover and outlines
 * what stays visible. Drawn in scene pixels so it scales with the preview.
 */
export function SafeAreaOverlay({
	width,
	height,
	safeArea,
	label,
}: {
	width: number;
	height: number;
	safeArea: SafeArea;
	label: string;
}) {
	const top = Math.round(height * safeArea.top);
	const bottom = Math.round(height * safeArea.bottom);
	const left = Math.round(width * safeArea.left);
	const right = Math.round(width * safeArea.right);
	const innerWidth = Math.max(0, width - left - right);
	const innerHeight = Math.max(0, height - top - bottom);
	const stroke = Math.max(2, Math.round(Math.min(width, height) / 270));
	const fontSize = Math.max(12, Math.round(Math.min(width, height) / 30));

	return (
		<svg
			width="100%"
			height="100%"
			viewBox={`0 0 ${width} ${height}`}
			xmlns="http://www.w3.org/2000/svg"
			role="img"
			aria-label={`${label} safe area`}
			data-testid="safe-area-overlay"
		>
			<title>{label} safe area</title>
			<g fill="rgba(0, 0, 0, 0.45)">
				<rect x={0} y={0} width={width} height={top} />
				<rect x={0} y={height - bottom} width={width} height={bottom} />
				<rect x={0} y={top} width={left} height={innerHeight} />
				<rect x={width - right} y={top} width={right} height={innerHeight} />
			</g>
			<rect
				x={left}
				y={top}
				width={innerWidth}
				height={innerHeight}
				fill="none"
				stroke="rgba(255, 255, 255, 0.9)"
				strokeWidth={stroke}
				strokeDasharray={`${stroke * 6} ${stroke * 4}`}
			/>
			<text
				x={left + stroke * 4}
				y={top + fontSize + stroke * 3}
				fill="rgba(255, 255, 255, 0.9)"
				fontSize={fontSize}
				fontFamily="system-ui, sans-serif"
			>
				{label} safe area (approximate)
			</text>
			<text
				x={width / 2}
				y={Math.max(fontSize, top / 2 + fontSize / 3)}
				fill="rgba(255, 255, 255, 0.75)"
				fontSize={fontSize}
				fontFamily="system-ui, sans-serif"
				textAnchor="middle"
			>
				Top {Math.round(safeArea.top * 100)}%
			</text>
			<text
				x={width / 2}
				y={height - bottom / 2 + fontSize / 3}
				fill="rgba(255, 255, 255, 0.75)"
				fontSize={fontSize}
				fontFamily="system-ui, sans-serif"
				textAnchor="middle"
			>
				Bottom {Math.round(safeArea.bottom * 100)}%
			</text>
		</svg>
	);
}

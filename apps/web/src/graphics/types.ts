import type { ParamDefinition, ParamValues } from "@/params";

export const DEFAULT_GRAPHIC_SOURCE_SIZE = 512;

export interface GraphicRenderContext {
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
	params: ParamValues;
	width: number;
	height: number;
}

export interface GraphicDefinition {
	id: string;
	name: string;
	keywords: string[];
	params: ParamDefinition[];
	render(context: GraphicRenderContext): void;
	/** Not offered among the shapes (a freehand drawing is made by drawing). */
	hidden?: boolean;
	/**
	 * The size it is drawn at for a canvas, when not the 512 square (a
	 * drawing covers the canvas).
	 */
	sourceSize?: (canvas: { width: number; height: number }) => { width: number; height: number };
	/**
	 * Params worked out at a time into the element (a drawing's reveal), put
	 * in front of the render; kept steady once it stops changing so the
	 * picture is cached.
	 */
	timedParams?: (params: ParamValues, elapsedSeconds: number) => ParamValues;
}

export interface GraphicInstance {
	definitionId: string;
	params: ParamValues;
}

import { BaseNode, type AnyBaseNode } from "./base-node";
import type { TransitionParams } from "@/transitions/types";

/**
 * A transition between two clips. `from` and `to` are the clips' own nodes
 * built in "extended" mode (they resolve past their ends and before their
 * starts); they belong to this node alone, so resolving them never touches
 * the clips' regular nodes, which are suppressed inside the window instead.
 */
export interface TransitionNodeParams {
	name: string;
	params: TransitionParams;
	windowStart: number;
	windowEnd: number;
	from: AnyBaseNode | null;
	to: AnyBaseNode | null;
}

export interface ResolvedTransitionNodeState {
	/** 0 shows only `from`, 1 only `to`. */
	progress: number;
}

export class TransitionNode extends BaseNode<
	TransitionNodeParams,
	ResolvedTransitionNodeState
> {}

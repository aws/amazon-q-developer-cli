import type { YogaNode } from '../layout/yoga.js';
import type { ComponentProps } from '../types/props.js';
import type { NODE_TYPES } from '../text/constants.js';

/**
 * Valid node type values.
 */
export type NodeType = typeof NODE_TYPES[keyof typeof NODE_TYPES];

/**
 * Internal representation of a node in the Twinki component tree.
 * 
 * TwinkiNode bridges React's virtual DOM with Twinki's layout and rendering
 * system. Each node corresponds to a React element and maintains both
 * React-specific data and layout information via Yoga.
 */
export interface TwinkiNode {
	/** Component type (e.g., 'twinki-text', 'twinki-box') */
	type: NodeType;
	/** Component props from React */
	props: ComponentProps;
	/** Yoga layout node (null for text nodes) */
	yogaNode: YogaNode | null;
	/** Child nodes */
	children: TwinkiNode[];
	/** Parent node reference */
	parent: TwinkiNode | null;
	/** Text content for text nodes */
	textContent?: string;
	/** Reference to root container */
	rootContainer?: RootContainer;
	/** Region this node belongs to (for scoped rendering) */
	region?: RegionState;
}

/**
 * Tracks dirty/clean state for a render region.
 * Nodes inside a Region share a RegionState so that
 * commitUpdate can mark only the owning region dirty.
 */
export interface RegionState {
	/** Unique region identifier */
	id: string;
	/** Whether this region needs re-render */
	dirty: boolean;
	/** Cached rendered lines from last render */
	cachedLines: string[] | null;
	/** Last width used for rendering (invalidate on change) */
	lastWidth: number;
}

/**
 * Root container that holds the entire component tree.
 * 
 * The root container manages the top-level Yoga layout node and
 * provides a callback for triggering re-renders when the tree changes.
 */
export interface RootContainer {
	/** Root Yoga layout node */
	yogaNode: YogaNode;
	/** Top-level child nodes */
	children: TwinkiNode[];
	/** Callback to trigger re-render */
	onRender: () => void;
	/** All registered regions */
	regions?: Map<string, RegionState>;
}
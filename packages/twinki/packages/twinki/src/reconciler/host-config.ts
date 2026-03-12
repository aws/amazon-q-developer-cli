import ReactReconciler from 'react-reconciler';
import { applyYogaProps } from '../layout/yoga.js';
import { renderTree as renderTreeImpl } from '../renderer/tree-renderer.js';
import { createNode, createTextNode } from './node-factory.js';
import { appendChild, removeChild, insertBefore, setRootContainer, freeNodeTree } from './tree-ops.js';
import { NODE_TYPES, CONSTANTS, PROP_NAMES } from '../text/constants.js';
import type { TwinkiNode, RootContainer, NodeType } from './types.js';
import type { ComponentProps } from '../types/props.js';

/**
 * Hit-tests the layout tree at (x, y) and returns the deepest node
 * whose computed bounds contain the point, along with its props chain.
 * Coordinates are 0-based terminal columns/rows.
 * 
 * @param root - Root container to hit test
 * @param x - X coordinate (column)
 * @param y - Y coordinate (row)
 * @returns Hit node or null if no hit
 */
export function hitTest(root: RootContainer, x: number, y: number): TwinkiNode | null {
	let hit: TwinkiNode | null = null;
	function walk(node: TwinkiNode, absX: number, absY: number): void {
		if (!node.yogaNode) return;
		const l = node.yogaNode.getComputedLeft();
		const t = node.yogaNode.getComputedTop();
		const w = node.yogaNode.getComputedWidth();
		const h = node.yogaNode.getComputedHeight();
		const ax = absX + l;
		const ay = absY + t;
		if (x >= ax && x < ax + w && y >= ay && y < ay + h) {
			hit = node;
			for (const child of node.children) walk(child, ax, ay);
		}
	}
	for (const child of root.children) {
		if (child.type !== NODE_TYPES.TWINKI_STATIC) walk(child, CONSTANTS.ZERO_INDEX, CONSTANTS.ZERO_INDEX);
	}
	return hit;
}

/**
 * Walks from a node up to root, returning the first node whose props
 * contain the given key.
 * 
 * @param node - Starting node to walk up from
 * @param prop - Property key to search for
 * @returns First ancestor node with the property, or null if not found
 */
export function findAncestorWithProp(node: TwinkiNode | null, prop: string): TwinkiNode | null {
	while (node) {
		if ((node.props as any)[prop]) return node;
		node = node.parent;
	}
	return null;
}

/**
 * Renders the React component tree to terminal output lines.
 * 
 * @param root - Root container to render
 * @param width - Terminal width for layout calculation
 * @param skipStaticItems - Number of static items already written (to avoid duplicates)
 * @returns Object with separated static and live content lines
 */
export function renderTree(root: RootContainer, width: number, skipStaticItems = 0): { staticLines: string[]; liveLines: string[] } {
	return renderTreeImpl(root, width, skipStaticItems);
}

/**
 * Finds the nearest Region ancestor of a node and marks it dirty.
 * If no region is found, marks the root container for full re-render.
 */
function markRegionDirty(node: TwinkiNode): void {
	let p: TwinkiNode | null = node;
	while (p) {
		if (p.region) {
			p.region.dirty = true;
			return;
		}
		p = p.parent;
	}
}

// --- Reconciler ---

/**
 * No-operation function used as a callback placeholder.
 */
const noop = (): void => {};

/**
 * React reconciler instance configured for Twinki's component system.
 * 
 * This reconciler bridges React's virtual DOM with Twinki's terminal
 * rendering system. It handles:
 * - Creating and managing TwinkiNode instances
 * - Mapping React props to Yoga layout properties
 * - Managing component lifecycle and updates
 * - Coordinating with the Yoga layout engine
 * 
 * The reconciler uses mutation mode for efficient updates and supports
 * all standard React features including hooks, context, and suspense.
 */
let currentUpdatePriority: number = CONSTANTS.ZERO_INDEX;

export const reconciler = ReactReconciler(({
	supportsMutation: true,
	supportsPersistence: false,
	isPrimaryRenderer: true,

	createInstance(type: NodeType, props: ComponentProps): TwinkiNode {
		return createNode(type, props);
	},

	createTextInstance(text: string): TwinkiNode {
		return createTextNode(text);
	},

	appendInitialChild(parent: TwinkiNode, child: TwinkiNode): void {
		parent.children.push(child);
		child.parent = parent;
		if (child.yogaNode && parent.type !== NODE_TYPES.TWINKI_TEXT) {
			parent.yogaNode!.insertChild(child.yogaNode, parent.yogaNode!.getChildCount());
		}
		if (parent.rootContainer) setRootContainer(child, parent.rootContainer);
	},

	appendChild(parent: TwinkiNode, child: TwinkiNode): void {
		appendChild(parent, child);
	},

	appendChildToContainer(container: RootContainer, child: TwinkiNode): void {
		appendChild(container, child);
	},

	insertBefore(parent: TwinkiNode, child: TwinkiNode, before: TwinkiNode): void {
		insertBefore(parent, child, before);
	},

	insertInContainerBefore(container: RootContainer, child: TwinkiNode, before: TwinkiNode): void {
		insertBefore(container, child, before);
	},

	removeChild(parent: TwinkiNode, child: TwinkiNode): void {
		removeChild(parent, child);
	},

	removeChildFromContainer(container: RootContainer, child: TwinkiNode): void {
		removeChild(container, child);
	},

	commitUpdate(instance: TwinkiNode, _type: NodeType, oldProps: ComponentProps, newProps: ComponentProps): void {
		instance.props = newProps;
		if (instance.yogaNode) {
			applyYogaProps(instance.yogaNode, newProps);
			if (instance.type === NODE_TYPES.TWINKI_TEXT) instance.yogaNode.markDirty();
		}
		// Mark all ancestor twinki-text nodes dirty
		let p = instance.parent;
		while (p?.type === NODE_TYPES.TWINKI_TEXT) {
			if (p.yogaNode && p.yogaNode.getChildCount() === CONSTANTS.ZERO_INDEX) p.yogaNode.markDirty();
			p = p.parent;
		}
		markRegionDirty(instance);
		if (instance.rootContainer) instance.rootContainer.onRender();
	},

	commitTextUpdate(instance: TwinkiNode, _oldText: string, newText: string): void {
		instance.textContent = newText;
		// Mark all ancestor twinki-text nodes dirty so Yoga re-measures
		let p = instance.parent;
		while (p) {
			if (p.yogaNode && p.type === NODE_TYPES.TWINKI_TEXT && p.yogaNode.getChildCount() === CONSTANTS.ZERO_INDEX) {
				p.yogaNode.markDirty();
			}
			if (p.type !== NODE_TYPES.TWINKI_TEXT) break;
			p = p.parent;
		}
		markRegionDirty(instance);
		if (instance.rootContainer) instance.rootContainer.onRender();
	},

	finalizeInitialChildren(): boolean {
		return false;
	},

	prepareUpdate(_instance: TwinkiNode, _type: NodeType, oldProps: ComponentProps, newProps: ComponentProps): Record<string, unknown> | null {
		// Return null if props haven't changed to avoid unnecessary commitUpdate calls
		if (oldProps === newProps) return null;
		const oldKeys = Object.keys(oldProps);
		const newKeys = Object.keys(newProps);
		if (oldKeys.length !== newKeys.length) return {};
		for (const key of newKeys) {
			if (key === PROP_NAMES.CHILDREN) continue;
			if ((oldProps as any)[key] !== (newProps as any)[key]) return {};
		}
		return null;
	},

	shouldSetTextContent(): boolean {
		return false;
	},

	getRootHostContext(): { isInsideText: boolean } {
		return { isInsideText: false };
	},

	getChildHostContext(parentContext: { isInsideText: boolean }): { isInsideText: boolean } {
		return parentContext;
	},

	getPublicInstance(instance: TwinkiNode): TwinkiNode {
		return instance;
	},

	prepareForCommit(container: RootContainer): null {
		return null;
	},

	resetAfterCommit(container: RootContainer): void {
		container.onRender();
	},

	preparePortalMount: noop,
	scheduleTimeout: setTimeout,
	cancelTimeout: clearTimeout,
	noTimeout: CONSTANTS.NO_TIMEOUT,
	supportsMicrotasks: true,
	scheduleMicrotask: queueMicrotask,

	getCurrentEventPriority(): number {
		return CONSTANTS.DEFAULT_EVENT_PRIORITY;
	},

	getInstanceFromNode(): null {
		return null;
	},

	setCurrentUpdatePriority(newPriority: number): void {
		currentUpdatePriority = newPriority;
	},
	getCurrentUpdatePriority: (): number => currentUpdatePriority,
	resolveUpdatePriority(): number {
		if (currentUpdatePriority !== CONSTANTS.ZERO_INDEX) {
			return currentUpdatePriority;
		}
		return CONSTANTS.DEFAULT_EVENT_PRIORITY;
	},

	trackSchedulerEvent(): void {},
	resolveEventType(): null {
		return null;
	},
	resolveEventTimeStamp(): number {
		return CONSTANTS.INVALID_TIMESTAMP;
	},

	beforeActiveInstanceBlur: noop,
	afterActiveInstanceBlur: noop,
	prepareScopeUpdate: noop,
	getInstanceFromScope(): null {
		return null;
	},

	detachDeletedInstance(instance: TwinkiNode): void {
		instance.rootContainer = undefined;
	},

	clearContainer(container: RootContainer): void {
		for (const child of container.children) {
			if (child.yogaNode) {
				container.yogaNode.removeChild(child.yogaNode);
			}
			freeNodeTree(child);
		}
		container.children = [];
	},

	supportsHydration: false,
}) as any);

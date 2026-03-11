import { NODE_TYPES, CONSTANTS, PROP_NAMES } from '../text/constants.js';
import type { TwinkiNode, RootContainer } from './types.js';

/**
 * Sets the root container reference for a node and all its descendants.
 * 
 * @param node - Node to update
 * @param rc - Root container reference
 */
export function setRootContainer(node: TwinkiNode, rc: RootContainer): void {
	node.rootContainer = rc;
	for (const child of node.children) setRootContainer(child, rc);
}

/**
 * Recursively frees Yoga nodes in a node tree to prevent memory leaks.
 * 
 * @param node - Root node of tree to free
 */
export function freeNodeTree(node: TwinkiNode): void {
	for (const child of node.children) freeNodeTree(child);
	if (node.yogaNode) {
		node.yogaNode.free();
		node.yogaNode = null;
	}
}

/**
 * Determines if a child should be added to the Yoga layout tree.
 * 
 * Text nodes are not added to Yoga as they're handled by their parent.
 * 
 * @param child - Child node to check
 * @returns Whether to add to Yoga tree
 */
export function shouldAddToYogaTree(child: TwinkiNode): boolean {
	return child.type !== NODE_TYPES.TEXT;
}

/**
 * Appends a child node to a parent node or root container.
 * 
 * Handles both regular nodes and root containers, managing the Yoga
 * layout tree and parent-child relationships appropriately.
 * 
 * @param parent - Parent node or root container
 * @param child - Child node to append
 */
export function appendChild(parent: TwinkiNode | RootContainer, child: TwinkiNode): void {
	// Detach from current parent first (like ink's appendChildNode)
	if (child.parent) {
		const oldParent = child.parent;
		const oldIdx = oldParent.children.indexOf(child);
		if (oldIdx !== CONSTANTS.SINGLE_UNIT * -1) {
			oldParent.children.splice(oldIdx, CONSTANTS.SINGLE_UNIT);
			if (child.yogaNode && oldParent.type !== NODE_TYPES.TWINKI_TEXT) {
				oldParent.yogaNode?.removeChild(child.yogaNode);
			}
		}
		child.parent = null;
	}

	const isTextParent = PROP_NAMES.TYPE in parent && parent.type === NODE_TYPES.TWINKI_TEXT;
	if (PROP_NAMES.TYPE in parent) {
		child.parent = parent;
		parent.children.push(child);
		if (child.yogaNode && !isTextParent) {
			parent.yogaNode!.insertChild(child.yogaNode, parent.yogaNode!.getChildCount());
		} else if (isTextParent && parent.yogaNode) {
			parent.yogaNode.markDirty();
		}
		if (parent.rootContainer) setRootContainer(child, parent.rootContainer);
	} else {
		child.parent = null;
		parent.children.push(child);
		if (child.yogaNode) {
			parent.yogaNode.insertChild(child.yogaNode, parent.yogaNode.getChildCount());
		}
		setRootContainer(child, parent);
	}
}

/**
 * Removes a child node from its parent.
 * 
 * Properly cleans up Yoga layout nodes and parent-child relationships.
 * Frees memory by calling freeNodeTree on removed nodes.
 * 
 * @param parent - Parent node or root container
 * @param child - Child node to remove
 */
export function removeChild(parent: TwinkiNode | RootContainer, child: TwinkiNode): void {
	const container = PROP_NAMES.TYPE in parent ? parent : parent;
	const isTextParent = PROP_NAMES.TYPE in parent && parent.type === NODE_TYPES.TWINKI_TEXT;
	const idx = container.children.indexOf(child);
	if (idx !== CONSTANTS.SINGLE_UNIT * -1) {
		container.children.splice(idx, CONSTANTS.SINGLE_UNIT);
		if (child.yogaNode && !isTextParent) {
			container.yogaNode!.removeChild(child.yogaNode);
			freeNodeTree(child);
		} else if (isTextParent && (parent as TwinkiNode).yogaNode) {
			(parent as TwinkiNode).yogaNode!.markDirty();
		}
	}
	child.parent = null;
	child.rootContainer = undefined;
}

/**
 * Inserts a child node before another child in the parent's children list.
 * 
 * Maintains proper ordering in both the React tree and Yoga layout tree.
 * Handles index calculation for Yoga nodes correctly.
 * 
 * @param parent - Parent node or root container
 * @param child - Child node to insert
 * @param before - Reference child to insert before
 */
export function insertBefore(parent: TwinkiNode | RootContainer, child: TwinkiNode, before: TwinkiNode): void {
	const container = PROP_NAMES.TYPE in parent ? parent : parent;
	const isTextParent = PROP_NAMES.TYPE in parent && parent.type === NODE_TYPES.TWINKI_TEXT;

	// Detach from current parent first (like ink's insertBeforeNode).
	// Don't use removeChild — it frees the yoga tree. Just detach.
	if (child.parent || container.children.includes(child)) {
		const oldParent = child.parent ?? container;
		const oldIdx = oldParent.children.indexOf(child);
		if (oldIdx !== CONSTANTS.SINGLE_UNIT * -1) {
			oldParent.children.splice(oldIdx, CONSTANTS.SINGLE_UNIT);
			if (child.yogaNode) {
				oldParent.yogaNode?.removeChild(child.yogaNode);
			}
		}
		child.parent = null;
	}

	const idx = container.children.indexOf(before);
	if (idx !== CONSTANTS.SINGLE_UNIT * -1) {
		child.parent = 'type' in parent ? parent : null;
		container.children.splice(idx, CONSTANTS.ZERO_INDEX, child);
		if (child.yogaNode && !isTextParent) {
			let yogaIdx = CONSTANTS.ZERO_INDEX;
			for (let i = CONSTANTS.ZERO_INDEX; i < idx; i++) {
				if (container.children[i]!.yogaNode) yogaIdx++;
			}
			container.yogaNode!.insertChild(child.yogaNode, yogaIdx);
		} else if (isTextParent && (parent as TwinkiNode).yogaNode) {
			(parent as TwinkiNode).yogaNode!.markDirty();
		}
		if (PROP_NAMES.TYPE in parent) {
			if (parent.rootContainer) setRootContainer(child, parent.rootContainer);
		} else {
			setRootContainer(child, parent);
		}
	}
}
import { Yoga, getComputedLayout } from '../layout/yoga.js';
import { renderText } from './text-renderer.js';
import { renderBox } from './box-renderer.js';
import { collectText } from '../text/text-processor.js';
import { stylize } from '../text/ansi-handler.js';
import { NODE_TYPES, CONSTANTS } from '../text/constants.js';
import type { TwinkiNode, RootContainer } from '../reconciler/types.js';

/**
 * Renders a single node to an array of terminal lines.
 * 
 * This is the main rendering dispatch function that handles different
 * node types and delegates to specialized rendering functions.
 * 
 * @param node - Node to render
 * @param maxWidth - Maximum available width
 * @returns Array of terminal lines
 */
export function renderNode(node: TwinkiNode, maxWidth: number): string[] {
	if (node.type === NODE_TYPES.TEXT) {
		return node.textContent ? node.textContent.split('\n') : [];
	}

	if (!node.yogaNode) return [];

	const layout = getComputedLayout(node.yogaNode);
	const width = Math.floor(layout.width);
	const height = Math.floor(layout.height);

	if (width <= CONSTANTS.ZERO_INDEX && node.type !== NODE_TYPES.TWINKI_TEXT) return [];

	// Region caching: return cached lines if region is clean
	if (node.type === NODE_TYPES.TWINKI_REGION && node.region) {
		if (!node.region.dirty && node.region.cachedLines && node.region.lastWidth === width) {
			return node.region.cachedLines;
		}
		const lines = renderChildren(node, width);
		node.region.cachedLines = lines;
		node.region.lastWidth = width;
		node.region.dirty = false;
		return lines;
	}

	if (node.type === NODE_TYPES.TWINKI_TEXT) {
		return renderText(node, width > CONSTANTS.ZERO_INDEX ? width : maxWidth);
	}

	if (node.type === NODE_TYPES.TWINKI_BOX) {
		return renderBox(node, width, height, (childNode, childMaxWidth) => 
			renderNode(childNode, childMaxWidth)
		);
	}

	if (node.type === NODE_TYPES.TWINKI_NEWLINE) {
		const count = (node.props as any).count ?? CONSTANTS.SINGLE_UNIT;
		return Array(count).fill('');
	}

	if (node.type === NODE_TYPES.TWINKI_SPACER) {
		return [];
	}

	if (node.type === NODE_TYPES.TWINKI_TRANSFORM) {
		const childLines = renderChildren(node, width);
		const transform = (node.props as any).transform;
		if (typeof transform === 'function') {
			return childLines.map((line) => transform(line));
		}
		return childLines;
	}

	return renderChildren(node, width);
}

/**
 * Renders all child nodes of a parent node.
 * 
 * @param node - Parent node whose children to render
 * @param width - Available width for rendering
 * @returns Array of terminal lines from all children
 */
export function renderChildren(node: TwinkiNode, width: number): string[] {
	const lines: string[] = [];
	for (const child of node.children) {
		lines.push(...renderNode(child, width));
	}
	return lines;
}

/**
 * Renders the React component tree to terminal output lines.
 * 
 * This is the main entry point for rendering that:
 * 1. Performs Yoga layout calculation
 * 2. Separates static content (for scrollback) from live content
 * 3. Renders all components to their final terminal representation
 * 
 * Static content is written to the terminal's scrollback buffer,
 * while live content forms the interactive area that gets updated.
 * 
 * @param root - Root container to render
 * @param width - Terminal width for layout calculation
 * @param skipStaticItems - Number of static items already written (to avoid duplicates)
 * @returns Object with separated static and live content lines
 */
export function renderTree(
	root: RootContainer, 
	width: number, 
	skipStaticItems = 0
): { staticLines: string[]; liveLines: string[] } {
	const validWidth = typeof width === "number" && !isNaN(width) && width > 0 ? width : 80; 
	root.yogaNode.setWidth(validWidth);
	root.yogaNode.calculateLayout(validWidth, undefined, Yoga.DIRECTION_LTR);
	const staticLines: string[] = [];
	const liveLines: string[] = [];
	
	// Find twinki-static node anywhere in the tree
	const findStatic = (node: TwinkiNode | RootContainer): TwinkiNode | null => {
		for (const child of node.children) {
			if (child.type === NODE_TYPES.TWINKI_STATIC) return child;
			const found = findStatic(child);
			if (found) return found;
		}
		return null;
	};
	const staticNode = findStatic(root);
	
	// Render static content (new items only)
	// Static nodes have DISPLAY_NONE so their children have 0 layout dimensions.
	// We need to render them with the full width.
	if (staticNode) {
		const toRender = skipStaticItems > 0 ? staticNode.children.slice(skipStaticItems) : staticNode.children;
		for (const sc of toRender) {
			// Force layout calculation for static children with full width
			if (sc.yogaNode) {
				sc.yogaNode.setWidth(validWidth);
				sc.yogaNode.calculateLayout(validWidth, undefined, Yoga.DIRECTION_LTR);
			}
			staticLines.push(...renderNode(sc, validWidth));
		}

		// Free Yoga nodes of already-flushed static children to prevent Wasm memory growth.
		// Safe because: (a) renderNode guards against null yogaNode, (b) on resize we use
		// accumulatedStaticOutput rather than re-rendering from Yoga (see resetStatic).
		for (let i = 0; i < skipStaticItems && i < staticNode.children.length; i++) {
			const child = staticNode.children[i]!;
			if (child.yogaNode) {
				staticNode.yogaNode!.removeChild(child.yogaNode);
				child.yogaNode.free();
				child.yogaNode = null;
			}
		}
	}

	// Render live content
	for (const child of root.children) {
		liveLines.push(...renderNode(child, validWidth));
	}
	
	return { staticLines, liveLines };
}
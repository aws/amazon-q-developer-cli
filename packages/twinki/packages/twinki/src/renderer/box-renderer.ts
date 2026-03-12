import { Yoga, getComputedLayout, getBorderChars } from '../layout/yoga.js';
import { colorToAnsi } from '../utils/color-parser.js';
import { visibleWidth } from '../utils/visible-width.js';
import { sliceByColumn } from '../utils/slice.js';
import { CONSTANTS, PROP_NAMES } from '../text/constants.js';
import type { TwinkiNode } from '../reconciler/types.js';

/**
 * Type for the renderNode function that will be injected
 */
type RenderNodeFn = (node: TwinkiNode, maxWidth: number) => string[];

/**
 * Renders box children with position-aware compositing.
 * 
 * Handles both simple column layout (vertical stacking) and complex
 * position-based compositing for row layouts and absolute positioning.
 * 
 * @param node - Box node containing children
 * @param innerWidth - Available inner width
 * @param innerHeight - Available inner height  
 * @param clipOverflow - Whether to clip content that exceeds bounds
 * @param contentOffsetLeft - Left offset for content positioning
 * @param contentOffsetTop - Top offset for content positioning
 * @param renderNodeFn - Function to render individual nodes
 * @returns Array of terminal lines representing the composed content
 */
export function renderBoxChildren(
	node: TwinkiNode, 
	innerWidth: number, 
	innerHeight: number, 
	clipOverflow: boolean, 
	contentOffsetLeft = 0, 
	contentOffsetTop = 0,
	renderNodeFn: RenderNodeFn
): string[] {
	// Check if any child has non-zero left/top (row layout or absolute positioning)
	let needsComposite = false;
	for (const child of node.children) {
		if (!child.yogaNode) continue;
		const layout = getComputedLayout(child.yogaNode);
		if (Math.floor(layout.left) - contentOffsetLeft !== 0 || Math.floor(layout.top) - contentOffsetTop !== 0) {
			needsComposite = true;
			break;
		}
	}

	if (!needsComposite) {
		// Column layout: simple vertical concatenation
		const lines: string[] = [];
		for (const child of node.children) {
			lines.push(...renderNodeFn(child, innerWidth));
		}
		if (clipOverflow && lines.length > innerHeight) return lines.slice(0, innerHeight);
		while (lines.length < innerHeight) lines.push('');
		return lines;
	}

	// Position-based compositing (row layout, etc.)
	const grid: string[] = new Array(innerHeight).fill('');

	for (const child of node.children) {
		if (!child.yogaNode) continue;
		const layout = getComputedLayout(child.yogaNode);
		const childLeft = Math.floor(layout.left) - contentOffsetLeft;
		const childTop = Math.floor(layout.top) - contentOffsetTop;
		const childLines = renderNodeFn(child, innerWidth);

		for (let i = 0; i < childLines.length; i++) {
			const row = childTop + i;
			if (row < 0 || row >= innerHeight) continue;
			const line = childLines[i]!;
			const base = grid[row]!;
			// Composite child line at childLeft offset
			const baseW = visibleWidth(base);
			let result = base;
			// Pad base to reach childLeft if needed
			if (baseW < childLeft) {
				result += ' '.repeat(childLeft - baseW);
			} else if (baseW > childLeft) {
				result = sliceByColumn(result, 0, childLeft);
			}
			result += line;
			// Truncate to innerWidth
			if (visibleWidth(result) > innerWidth) {
				result = sliceByColumn(result, 0, innerWidth);
			}
			grid[row] = result;
		}
	}

	return grid;
}

/**
 * Renders a box node with borders, padding, and background.
 * 
 * Handles:
 * - Border rendering with configurable styles and colors
 * - Padding application (top, bottom, left, right)
 * - Background color application
 * - Content positioning and compositing
 * - Overflow clipping when enabled
 * 
 * @param node - Box node to render
 * @param width - Total box width
 * @param height - Total box height
 * @param renderNodeFn - Function to render individual nodes
 * @returns Array of terminal lines representing the rendered box
 */
/**
 * Calculates border and padding dimensions for a box.
 */
function calculateBoxDimensions(node: TwinkiNode, width: number, height: number, hasBorder: boolean) {
	const pTop = node.yogaNode!.getComputedPadding(Yoga.EDGE_TOP);
	const pBottom = node.yogaNode!.getComputedPadding(Yoga.EDGE_BOTTOM);
	const pLeft = node.yogaNode!.getComputedPadding(Yoga.EDGE_LEFT);
	const pRight = node.yogaNode!.getComputedPadding(Yoga.EDGE_RIGHT);

	const borderW = hasBorder ? 1 : 0;
	const innerWidth = Math.max(0, width - pLeft - pRight - borderW * 2);
	const innerHeight = Math.max(0, height - pTop - pBottom - borderW * 2);

	return { pTop, pBottom, pLeft, pRight, borderW, innerWidth, innerHeight };
}

/**
 * Renders border and padding lines for a box.
 */
function renderBoxFrame(
	width: number,
	border: any,
	borderColor: string,
	borderReset: string,
	bgCode: string,
	bgReset: string,
	pTop: number,
	pBottom: number,
	borderW: number,
	content: string[]
) {
	const lines: string[] = [];

	// Top border
	if (border) {
		lines.push(borderColor + border.topLeft + border.horizontal.repeat(width - 2) + border.topRight + borderReset);
	}

	// Top padding
	for (let i = 0; i < pTop; i++) {
		const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
			' '.repeat(width - borderW * 2) +
			(border ? borderColor + border.vertical + borderReset : '') + bgReset;
		lines.push(padLine);
	}

	// Content lines
	lines.push(...content);

	// Bottom padding
	for (let i = 0; i < pBottom; i++) {
		const padLine = bgCode + (border ? borderColor + border.vertical + borderReset : '') +
			' '.repeat(width - borderW * 2) +
			(border ? borderColor + border.vertical + borderReset : '') + bgReset;
		lines.push(padLine);
	}

	// Bottom border
	if (border) {
		lines.push(borderColor + border.bottomLeft + border.horizontal.repeat(width - 2) + border.bottomRight + borderReset);
	}

	return lines;
}

export function renderBox(node: TwinkiNode, width: number, height: number, renderNodeFn: RenderNodeFn): string[] {
	const props = node.props as any;
	const hasBorder = !!props.borderStyle;
	const border = hasBorder ? getBorderChars(props.borderStyle) : null;

	const { pTop, pBottom, pLeft, pRight, borderW, innerWidth, innerHeight } = 
		calculateBoxDimensions(node, width, height, hasBorder);

	// Render children with position-aware compositing
	const childContent = renderBoxChildren(
		node, 
		innerWidth, 
		innerHeight, 
		props.overflow === PROP_NAMES.HIDDEN, 
		borderW + pLeft, 
		borderW + pTop,
		renderNodeFn
	);

	// Apply colors
	const bgCode = props.backgroundColor ? `\x1b[${colorToAnsi(props.backgroundColor, true)}m` : '';
	const bgReset = bgCode ? '\x1b[0m' : '';
	const borderColor = props.borderColor ? `\x1b[${colorToAnsi(props.borderColor, false)}m` : '';
	const borderReset = borderColor ? '\x1b[0m' : '';

	// Format content lines with padding
	const leftPad = ' '.repeat(pLeft);
	const rightPad = ' '.repeat(pRight);
	const content = childContent.map(line => {
		const lineW = visibleWidth(line);
		const fill = Math.max(0, innerWidth - lineW);
		return bgCode +
			(border ? borderColor + border.vertical + borderReset : '') +
			leftPad + line + ' '.repeat(fill) + rightPad +
			(border ? borderColor + border.vertical + borderReset : '') + bgReset;
	});

	return renderBoxFrame(width, border, borderColor, borderReset, bgCode, bgReset, pTop, pBottom, borderW, content);
}
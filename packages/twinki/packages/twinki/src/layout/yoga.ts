import Yoga, { type Node as YogaNode } from 'yoga-layout';
import type { ComponentProps } from '../types/props.js';
import { PROP_NAMES } from '../text/constants.js';

export { Yoga, type YogaNode };

/**
 * Creates a new Yoga layout node.
 * 
 * Yoga nodes are used for flexbox-based layout calculations in the terminal.
 * Each node represents a layout container or element that can have dimensions,
 * positioning, and flex properties.
 * 
 * @returns New Yoga node instance
 */
export function createYogaNode(): YogaNode {
	return Yoga.Node.create();
}

/**
 * Applies layout properties to a Yoga node.
 * 
 * This function maps CSS-like properties to Yoga layout properties,
 * handling various property formats and providing sensible defaults.
 * Supports:
 * - Dimensions (width, height, min/max sizes)
 * - Flexbox properties (direction, grow, shrink, wrap)
 * - Alignment (alignItems, alignSelf, justifyContent)
 * - Spacing (padding, margin)
 * - Border and overflow
 * - Display modes
 * 
 * @param node - Yoga node to configure
 * @param props - Object containing layout properties
 */
export function applyYogaProps(node: YogaNode, props: ComponentProps): void {
	// Dimensions
	if (props.width !== undefined) {
		if (typeof props.width === 'string' && props.width.endsWith('%')) {
			node.setWidthPercent(parseFloat(props.width));
		} else {
			node.setWidth(props.width as number);
		}
	}
	if (props.height !== undefined) {
		if (typeof props.height === 'string' && props.height.endsWith('%')) {
			node.setHeightPercent(parseFloat(props.height));
		} else {
			node.setHeight(props.height as number);
		}
	}
	if (props.minWidth !== undefined) node.setMinWidth(props.minWidth);
	if (props.minHeight !== undefined) node.setMinHeight(props.minHeight);

	// Flex
	if (props.flexDirection !== undefined) {
		const map: Record<string, number> = {
			row: Yoga.FLEX_DIRECTION_ROW,
			column: Yoga.FLEX_DIRECTION_COLUMN,
			'row-reverse': Yoga.FLEX_DIRECTION_ROW_REVERSE,
			'column-reverse': Yoga.FLEX_DIRECTION_COLUMN_REVERSE,
		};
		node.setFlexDirection(map[props.flexDirection] ?? Yoga.FLEX_DIRECTION_ROW);
	}
	if (props.flexGrow !== undefined) node.setFlexGrow(props.flexGrow);
	if (props.flexShrink !== undefined) node.setFlexShrink(props.flexShrink);
	if (props.flexBasis !== undefined) node.setFlexBasis(props.flexBasis as number);
	if (props.gap !== undefined) {
		node.setGap(Yoga.GUTTER_ALL, props.gap);
	}
	if (props.columnGap !== undefined) node.setGap(Yoga.GUTTER_COLUMN, props.columnGap);
	if (props.rowGap !== undefined) node.setGap(Yoga.GUTTER_ROW, props.rowGap);
	if (props.flexWrap !== undefined) {
		const map: Record<string, number> = {
			nowrap: Yoga.WRAP_NO_WRAP,
			wrap: Yoga.WRAP_WRAP,
			'wrap-reverse': Yoga.WRAP_WRAP_REVERSE,
		};
		node.setFlexWrap(map[props.flexWrap] ?? Yoga.WRAP_NO_WRAP);
	}

	// Alignment
	if (props.alignItems !== undefined) {
		node.setAlignItems(alignValue(props.alignItems));
	}
	if (props.alignSelf !== undefined) {
		node.setAlignSelf(alignValue(props.alignSelf));
	}
	if (props.justifyContent !== undefined) {
		const map: Record<string, number> = {
			'flex-start': Yoga.JUSTIFY_FLEX_START,
			center: Yoga.JUSTIFY_CENTER,
			'flex-end': Yoga.JUSTIFY_FLEX_END,
			'space-between': Yoga.JUSTIFY_SPACE_BETWEEN,
			'space-around': Yoga.JUSTIFY_SPACE_AROUND,
			'space-evenly': Yoga.JUSTIFY_SPACE_EVENLY,
		};
		node.setJustifyContent(map[props.justifyContent] ?? Yoga.JUSTIFY_FLEX_START);
	}

	// Padding
	applyEdges(node, 'setPadding', props, 'padding');
	// Margin
	applyEdges(node, 'setMargin', props, 'margin');

	// Border (Yoga uses border for layout calculation)
	if (props.borderStyle) {
		node.setBorder(Yoga.EDGE_ALL, 1);
	}

	// Overflow
	if (props.overflow === PROP_NAMES.HIDDEN) {
		node.setOverflow(Yoga.OVERFLOW_HIDDEN);
	}

	// Display
	if ((props as any).display === 'none') {
		node.setDisplay(Yoga.DISPLAY_NONE);
	}
}

/**
 * Maps alignment string values to Yoga alignment constants.
 * 
 * @param v - Alignment value string
 * @returns Yoga alignment constant
 */
function alignValue(v: string): number {
	const map: Record<string, number> = {
		'flex-start': Yoga.ALIGN_FLEX_START,
		center: Yoga.ALIGN_CENTER,
		'flex-end': Yoga.ALIGN_FLEX_END,
		stretch: Yoga.ALIGN_STRETCH,
	};
	return map[v] ?? Yoga.ALIGN_STRETCH;
}

/**
 * Applies edge-based properties (padding/margin) to a Yoga node.
 * 
 * Handles various property formats:
 * - Single value for all edges
 * - Specific edge values (Top, Bottom, Left, Right)
 * - Axis-based values (X for left/right, Y for top/bottom)
 * 
 * @param node - Yoga node to modify
 * @param method - Method name to call on the node
 * @param props - Properties object
 * @param prefix - Property prefix (e.g., 'padding', 'margin')
 */
function applyEdges(
	node: YogaNode,
	method: 'setPadding' | 'setMargin',
	props: ComponentProps,
	prefix: string,
): void {
	const propsAny = props as any;
	const all = propsAny[prefix];
	if (all !== undefined) node[method](Yoga.EDGE_ALL, all);
	const top = propsAny[`${prefix}Top`] ?? propsAny[`${prefix}Y`];
	if (top !== undefined) node[method](Yoga.EDGE_TOP, top);
	const bottom = propsAny[`${prefix}Bottom`] ?? propsAny[`${prefix}Y`];
	if (bottom !== undefined && propsAny[`${prefix}Bottom`] !== undefined) node[method](Yoga.EDGE_BOTTOM, bottom);
	else if (all === undefined && propsAny[`${prefix}Y`] !== undefined) node[method](Yoga.EDGE_BOTTOM, propsAny[`${prefix}Y`]);
	const left = propsAny[`${prefix}Left`] ?? propsAny[`${prefix}X`];
	if (left !== undefined) node[method](Yoga.EDGE_LEFT, left);
	const right = propsAny[`${prefix}Right`] ?? propsAny[`${prefix}X`];
	if (right !== undefined && propsAny[`${prefix}Right`] !== undefined) node[method](Yoga.EDGE_RIGHT, right);
	else if (all === undefined && propsAny[`${prefix}X`] !== undefined) node[method](Yoga.EDGE_RIGHT, propsAny[`${prefix}X`]);
}

/**
 * Gets the computed layout dimensions and position from a Yoga node.
 * 
 * After layout calculation, this function extracts the final computed
 * values for positioning and sizing the element in the terminal.
 * 
 * @param node - Yoga node with computed layout
 * @returns Object with left, top, width, and height values
 */
export function getComputedLayout(node: YogaNode): { left: number; top: number; width: number; height: number } {
	return {
		left: node.getComputedLeft(),
		top: node.getComputedTop(),
		width: node.getComputedWidth(),
		height: node.getComputedHeight(),
	};
}

/**
 * Border character sets for different visual styles.
 * 
 * Provides Unicode box-drawing characters for various border styles
 * used in terminal UI components.
 */
const BORDER_STYLES: Record<string, { topLeft: string; topRight: string; bottomLeft: string; bottomRight: string; horizontal: string; vertical: string }> = {
	single: { topLeft: '┌', topRight: '┐', bottomLeft: '└', bottomRight: '┘', horizontal: '─', vertical: '│' },
	double: { topLeft: '╔', topRight: '╗', bottomLeft: '╚', bottomRight: '╝', horizontal: '═', vertical: '║' },
	round: { topLeft: '╭', topRight: '╮', bottomLeft: '╰', bottomRight: '╯', horizontal: '─', vertical: '│' },
	bold: { topLeft: '┏', topRight: '┓', bottomLeft: '┗', bottomRight: '┛', horizontal: '━', vertical: '┃' },
	classic: { topLeft: '+', topRight: '+', bottomLeft: '+', bottomRight: '+', horizontal: '-', vertical: '|' },
	singleDouble: { topLeft: '╓', topRight: '╖', bottomLeft: '╙', bottomRight: '╜', horizontal: '─', vertical: '║' },
	doubleSingle: { topLeft: '╒', topRight: '╕', bottomLeft: '╘', bottomRight: '╛', horizontal: '═', vertical: '│' },
};

/**
 * Gets border characters for the specified style.
 * 
 * Returns the appropriate Unicode box-drawing characters for creating
 * borders in terminal UI components. Falls back to 'single' style
 * if the requested style is not found.
 * 
 * @param style - Border style name
 * @returns Object with border characters for corners and edges
 * 
 * @example
 * ```typescript
 * const chars = getBorderChars('double');
 * // chars.topLeft = '╔', chars.horizontal = '═', etc.
 * ```
 */
export function getBorderChars(style: string) {
	return BORDER_STYLES[style] ?? BORDER_STYLES.single!;
}

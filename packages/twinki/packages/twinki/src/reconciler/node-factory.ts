import { Yoga, createYogaNode, applyYogaProps } from '../layout/yoga.js';
import { visibleWidth } from '../utils/visible-width.js';
import { wrapTextWithAnsi } from '../utils/wrap-ansi.js';
import { collectText } from '../text/text-processor.js';
import { stylize } from '../text/ansi-handler.js';
import { NODE_TYPES, WrapMode, CONSTANTS } from '../text/constants.js';
import type { TwinkiNode, NodeType } from './types.js';
import type { ComponentProps } from '../types/props.js';

/**
 * Creates a new TwinkiNode with the specified type and props.
 * 
 * Sets up the Yoga layout node and applies layout properties from props.
 * Special handling for text nodes and static content.
 * 
 * @param type - Component type
 * @param props - Component props
 * @returns New TwinkiNode instance
 */
export function createNode(type: NodeType, props: ComponentProps): TwinkiNode {
	const yogaNode = createYogaNode();
	applyYogaProps(yogaNode, props);
	if (type === NODE_TYPES.TWINKI_STATIC) yogaNode.setDisplay(Yoga.DISPLAY_NONE);
	const node: TwinkiNode = { type, props, yogaNode, children: [], parent: null };
	if (type === NODE_TYPES.TWINKI_TEXT) setTextMeasureFunc(node);
	return node;
}

/**
 * Creates a text node with the specified content.
 * 
 * Text nodes don't have Yoga layout nodes as they're measured
 * and positioned by their parent text components.
 * 
 * @param text - Text content
 * @returns New text node
 */
export function createTextNode(text: string): TwinkiNode {
	return { 
		type: NODE_TYPES.TEXT, 
		props: {}, 
		yogaNode: null, 
		children: [], 
		parent: null, 
		textContent: text 
	};
}

/**
 * Sets up text measurement function for text nodes.
 * 
 * Text nodes need custom measurement logic that accounts for text wrapping,
 * ANSI codes, and various text formatting options. This function configures
 * the Yoga measure function to properly calculate text dimensions.
 * 
 * @param node - Text node to configure
 */
export function setTextMeasureFunc(node: TwinkiNode): void {
	if (node.type !== NODE_TYPES.TWINKI_TEXT || !node.yogaNode) return;
	node.yogaNode.setMeasureFunc((width, widthMode) => {
		const text = stylize(collectText(node, stylize), node.props);
		if (!text) return { width: 0, height: 0 };
		const maxW = widthMode === Yoga.MEASURE_MODE_UNDEFINED ? Infinity : width;
		const wrap = (node.props as any).wrap ?? WrapMode.WRAP;
		let lines: string[];
		if (wrap === WrapMode.WRAP) {
			lines = wrapTextWithAnsi(text, Math.max(1, Math.floor(maxW)));
		} else {
			lines = text.split('\n');
		}
		const measuredWidth = lines.reduce((max, l) => Math.max(max, visibleWidth(l)), 0);
		return { width: measuredWidth, height: lines.length };
	});
}
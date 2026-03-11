import { NODE_TYPES } from './constants.js';
import type { TwinkiNode } from '../reconciler/types.js';
import type { ComponentProps } from '../types/props.js';
import { sanitizeText } from './ansi-handler.js';

/**
 * Collects text content from a node and its children.
 * 
 * Recursively traverses the node tree to gather all text content,
 * applying styling from nested text components as it goes.
 * 
 * @param node - Node to collect text from
 * @param stylizeFn - Function to apply styling to text
 * @returns Collected text with ANSI styling
 */
export function collectText(node: TwinkiNode, stylizeFn: (text: string, props: ComponentProps) => string): string {
	if (node.textContent !== undefined) return sanitizeText(node.textContent);
	let result = '';
	for (const child of node.children) {
		if (child.type === NODE_TYPES.TEXT) {
			result += sanitizeText(child.textContent ?? '');
		} else if (child.type === NODE_TYPES.TWINKI_TEXT) {
			const inner = collectText(child, stylizeFn);
			if (inner) result += stylizeFn(inner, child.props);
		}
	}
	return result;
}
import { visibleWidth } from '../utils/visible-width.js';
import { wrapTextWithAnsi } from '../utils/wrap-ansi.js';
import { sliceByColumn } from '../utils/slice.js';
import { collectText } from '../text/text-processor.js';
import { stylize } from '../text/ansi-handler.js';
import { WrapMode } from '../text/constants.js';
import type { TwinkiNode } from '../reconciler/types.js';

const TRUNCATE_ELLIPSIS = '…';

/**
 * Renders a text node to an array of terminal lines.
 * 
 * Handles different text wrapping modes:
 * - wrap: Word wrapping with ANSI preservation
 * - truncate/truncate-end: Truncate at end with ellipsis
 * - truncate-start: Truncate at start
 * - truncate-middle: Truncate in middle with ellipsis
 * 
 * @param node - Text node to render
 * @param width - Available width for text
 * @returns Array of terminal lines
 */
export function renderText(node: TwinkiNode, width: number): string[] {
	const text = stylize(collectText(node, stylize), node.props);
	if (!text) return [];

	const wrap = (node.props as any).wrap ?? WrapMode.WRAP;
	if (wrap === WrapMode.WRAP) {
		return wrapTextWithAnsi(text, width);
	}
	if (wrap === WrapMode.TRUNCATE || wrap === WrapMode.TRUNCATE_END) {
		const lines = text.split('\n');
		return lines.map((line) => {
			if (visibleWidth(line) > width) return sliceByColumn(line, 0, width);
			return line;
		});
	}
	if (wrap === WrapMode.TRUNCATE_START) {
		const lines = text.split('\n');
		return lines.map((line) => {
			const w = visibleWidth(line);
			if (w > width) return sliceByColumn(line, w - width, width);
			return line;
		});
	}
	if (wrap === WrapMode.TRUNCATE_MIDDLE) {
		const lines = text.split('\n');
		return lines.map((line) => {
			const w = visibleWidth(line);
			if (w <= width) return line;
			const half = Math.floor((width - 1) / 2);
			const start = sliceByColumn(line, 0, half);
			const end = sliceByColumn(line, w - (width - half - 1), width - half - 1);
			return start + TRUNCATE_ELLIPSIS + end;
		});
	}
	return wrapTextWithAnsi(text, width);
}
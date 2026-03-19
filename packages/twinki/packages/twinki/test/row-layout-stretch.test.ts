import { describe, it, expect } from 'vitest';
import { Yoga, createYogaNode, applyYogaProps } from '../src/layout/yoga.js';
import { renderBoxChildren } from '../src/renderer/box-renderer.js';
import { createNode } from '../src/reconciler/node-factory.js';
import { NODE_TYPES } from '../src/text/constants.js';
import { renderNode } from '../src/renderer/tree-renderer.js';

/**
 * Helper: build a yoga tree and compute layout, then render.
 */
function buildAndRender(width: number) {
	// Simulate StreamingPanel layout:
	// Box(column, height=20)
	//   Box(row)
	//     Box(column, flexGrow=1) → content (5 lines of text)
	//     Text → scrollbar (1 col wide)
	//   Text → "scroll hint"

	const root = createNode(NODE_TYPES.TWINKI_BOX as any, {
		flexDirection: 'column',
		height: 20,
	});

	const row = createNode(NODE_TYPES.TWINKI_BOX as any, {
		flexDirection: 'row',
	});

	const contentCol = createNode(NODE_TYPES.TWINKI_BOX as any, {
		flexDirection: 'column',
		flexGrow: 1,
	});

	// 5 lines of content
	for (let i = 0; i < 5; i++) {
		const textNode = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
		textNode.textContent = `Line ${i + 1}: some content here`;
		contentCol.children.push(textNode);
		textNode.parent = contentCol;
		if (textNode.yogaNode) contentCol.yogaNode!.insertChild(textNode.yogaNode, contentCol.children.length - 1);
	}

	// Scrollbar (single column text)
	const scrollbar = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
	scrollbar.textContent = '▲\n█\n█\n░\n▼';

	// Assemble: row = [contentCol, scrollbar]
	row.children.push(contentCol);
	contentCol.parent = row;
	if (contentCol.yogaNode) row.yogaNode!.insertChild(contentCol.yogaNode, 0);

	row.children.push(scrollbar);
	scrollbar.parent = row;
	if (scrollbar.yogaNode) row.yogaNode!.insertChild(scrollbar.yogaNode, 1);

	// Hint text
	const hint = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
	hint.textContent = '  Fn+↑/↓ to scroll';

	// Assemble: root = [row, hint]
	root.children.push(row);
	row.parent = root;
	if (row.yogaNode) root.yogaNode!.insertChild(row.yogaNode, 0);

	root.children.push(hint);
	hint.parent = root;
	if (hint.yogaNode) root.yogaNode!.insertChild(hint.yogaNode, 1);

	// Compute layout
	root.yogaNode!.setWidth(width);
	root.yogaNode!.calculateLayout(width, undefined, Yoga.DIRECTION_LTR);

	return { root, row, contentCol };
}

describe('Row layout with fixed-height parent (StreamingPanel pattern)', () => {
	it('should not pad content with empty lines when content is shorter than parent height', () => {
		const { root } = buildAndRender(80);
		const lines = renderNode(root, 80);

		// Content is 5 lines + scrollbar 5 lines + hint 1 line
		// Should NOT be 20 lines (the fixed height of the parent)
		// Empty trailing lines = stretching bug
		const nonEmptyLines = lines.filter(l => l.trim().length > 0);

		// We expect roughly 5-6 content lines + 1 hint = ~6-7 non-empty lines
		// The key assertion: total lines should NOT be padded to 20
		expect(lines.length).toBeLessThanOrEqual(20);
		expect(nonEmptyLines.length).toBeGreaterThanOrEqual(5);
	});

	it('should not have blank lines between content lines', () => {
		const { root } = buildAndRender(80);
		const lines = renderNode(root, 80);

		// Find the content lines (Line 1 through Line 5)
		const contentLineIndices: number[] = [];
		for (let i = 0; i < lines.length; i++) {
			if (lines[i]!.includes('Line ')) {
				contentLineIndices.push(i);
			}
		}

		expect(contentLineIndices.length).toBe(5);

		// Content lines should be consecutive (no blank lines between them)
		for (let i = 1; i < contentLineIndices.length; i++) {
			const gap = contentLineIndices[i]! - contentLineIndices[i - 1]!;
			expect(gap).toBe(1);
		}
	});

	it('column layout without row should not stretch', () => {
		// Simple column Box with fixed height and 3 lines of text
		const box = createNode(NODE_TYPES.TWINKI_BOX as any, {
			flexDirection: 'column',
			height: 20,
		});

		for (let i = 0; i < 3; i++) {
			const text = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
			text.textContent = `Item ${i}`;
			box.children.push(text);
			text.parent = box;
			if (text.yogaNode) box.yogaNode!.insertChild(text.yogaNode, i);
		}

		box.yogaNode!.setWidth(80);
		box.yogaNode!.calculateLayout(80, undefined, Yoga.DIRECTION_LTR);

		const lines = renderNode(box, 80);
		// Should be 3 lines, not 20
		const nonEmpty = lines.filter(l => l.trim().length > 0);
		expect(nonEmpty.length).toBe(3);
	});
});

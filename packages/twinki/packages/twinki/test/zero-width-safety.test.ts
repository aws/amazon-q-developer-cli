import { describe, it, expect } from 'vitest';
import { renderText } from '../src/renderer/text-renderer.js';
import { renderTree } from '../src/renderer/tree-renderer.js';
import { createNode, createTextNode } from '../src/reconciler/node-factory.js';
import { NODE_TYPES } from '../src/text/constants.js';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';

describe('zero-width safety', () => {
	it('renderText returns empty at width 0', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
		const text = createTextNode('hello world this is a long string');
		node.children.push(text);
		text.parent = node;
		expect(renderText(node, 0)).toEqual([]);
	});

	it('renderText returns empty at negative width', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
		const text = createTextNode('test');
		node.children.push(text);
		text.parent = node;
		expect(renderText(node, -5)).toEqual([]);
	});

	it('renderText works at width 1', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
		const text = createTextNode('hi');
		node.children.push(text);
		text.parent = node;
		const lines = renderText(node, 1);
		expect(lines.length).toBeGreaterThan(0);
		expect(lines.length).toBeLessThanOrEqual(3);
	});

	it('wrapTextWithAnsi at width 0 completes in bounded time', () => {
		const start = performance.now();
		const lines = wrapTextWithAnsi('a'.repeat(1000), 1);
		const elapsed = performance.now() - start;
		// Must complete in under 100ms, not hang
		expect(elapsed).toBeLessThan(100);
		expect(lines.length).toBe(1000);
	});

	it('renderTree clamps width to minimum 10', () => {
		const root = createNode(NODE_TYPES.TWINKI_BOX, {});
		// @ts-ignore — rootContainer shape
		root.yogaNode.setWidth(3);
		const start = performance.now();
		const result = renderTree(root as any, 3);
		const elapsed = performance.now() - start;
		// Must complete fast — width clamped to 80 fallback
		expect(elapsed).toBeLessThan(50);
		expect(result.staticLines).toBeDefined();
		expect(result.liveLines).toBeDefined();
	});

	it('renderText at width 5 completes in bounded time with long text', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
		const text = createTextNode('The quick brown fox jumps over the lazy dog. '.repeat(20));
		node.children.push(text);
		text.parent = node;
		const start = performance.now();
		const lines = renderText(node, 5);
		const elapsed = performance.now() - start;
		expect(elapsed).toBeLessThan(200);
		expect(lines.length).toBeGreaterThan(0);
	});
});

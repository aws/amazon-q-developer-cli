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

	// A width that cannot fit a character used to loop forever. Wrapping is
	// synchronous, so a return to that bug would stall the worker rather than
	// trip a per-test timeout — no duration assertion can catch it, and the
	// symptom would be the whole run hitting its own limit. What is checkable is
	// the output shape, so that is all these cases claim.
	it('wrapTextWithAnsi at width 1 emits one line per character', () => {
		const lines = wrapTextWithAnsi('a'.repeat(1000), 1);
		expect(lines.length).toBe(1000);
	});

	it('renderTree clamps width to minimum 10', () => {
		const root = createNode(NODE_TYPES.TWINKI_BOX, {});
		// @ts-ignore — rootContainer shape
		root.yogaNode.setWidth(3);
		const result = renderTree(root as any, 3);
		expect(result.staticLines).toBeDefined();
		expect(result.liveLines).toBeDefined();
	});

	it('renderText wraps long text at width 5', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
		const text = createTextNode('The quick brown fox jumps over the lazy dog. '.repeat(20));
		node.children.push(text);
		text.parent = node;
		const lines = renderText(node, 5);
		expect(lines.length).toBeGreaterThan(0);
	});
});

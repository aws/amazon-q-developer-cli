import { describe, it, expect } from 'vitest';
import { renderTree, renderNode } from '../src/renderer/tree-renderer.js';
import { createNode, createTextNode } from '../src/reconciler/node-factory.js';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';
import { NODE_TYPES } from '../src/text/constants.js';
import type { RootContainer, TwinkiNode } from '../src/reconciler/types.js';

/**
 * Simulates the real-world scenario reported by Joshua Samuel:
 * - Large conversation tree (~8000+ nodes)
 * - Spinner animating at 100ms interval (only its text changes)
 * - Region should prevent full layout recalculation
 */

function buildLargeTree(messageCount: number): {
	root: RootContainer;
	spinnerText: TwinkiNode;
	spinnerRegion: TwinkiNode;
} {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

	const root: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	// Heavy conversation region (simulates chat messages)
	const chatRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'chat' });
	for (let i = 0; i < messageCount; i++) {
		const box = createNode(NODE_TYPES.TWINKI_BOX as any, { flexDirection: 'column' });
		const textNode = createNode(NODE_TYPES.TWINKI_TEXT as any, { wrap: 'wrap' });
		const rawText = createTextNode(`Message ${i}: ${'Lorem ipsum dolor sit amet. '.repeat(3)}`);
		textNode.children.push(rawText);
		rawText.parent = textNode;
		box.children.push(textNode);
		textNode.parent = box;
		if (textNode.yogaNode) box.yogaNode!.insertChild(textNode.yogaNode, box.yogaNode!.getChildCount());
		chatRegion.children.push(box);
		box.parent = chatRegion;
		if (box.yogaNode) chatRegion.yogaNode!.insertChild(box.yogaNode, chatRegion.yogaNode!.getChildCount());
	}
	root.children.push(chatRegion);
	chatRegion.parent = null;
	if (chatRegion.yogaNode) root.yogaNode.insertChild(chatRegion.yogaNode, root.yogaNode.getChildCount());

	// Spinner region (simulates the status bar spinner)
	const spinnerRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'spinner' });
	const spinnerText = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
	const spinnerRaw = createTextNode('⠋');
	spinnerText.children.push(spinnerRaw);
	spinnerRaw.parent = spinnerText;
	spinnerRegion.children.push(spinnerText);
	spinnerText.parent = spinnerRegion;
	if (spinnerText.yogaNode) spinnerRegion.yogaNode!.insertChild(spinnerText.yogaNode, spinnerRegion.yogaNode!.getChildCount());
	root.children.push(spinnerRegion);
	spinnerRegion.parent = null;
	if (spinnerRegion.yogaNode) root.yogaNode.insertChild(spinnerRegion.yogaNode, root.yogaNode.getChildCount());

	return { root, spinnerText, spinnerRegion };
}

describe('Region layout skip optimization', () => {
	it('isDirty guard: skips calculateLayout when only text content changes in a Region', () => {
		const { root, spinnerText, spinnerRegion } = buildLargeTree(500);
		const WIDTH = 80;
		const ITERATIONS = 100;

		// Initial render — layout is computed
		const result = renderTree(root, WIDTH);
		expect(result.liveLines.length).toBeGreaterThan(0);

		// Simulate spinner updates: only text changes, no layout-affecting changes
		// After initial render, yoga tree should be clean (not dirty)
		// The spinner text update marks the yoga node dirty via commitTextUpdate,
		// but ONLY the spinner's yoga node — not the entire tree.
		// With isDirty guard, calculateLayout should be fast because only
		// the spinner subtree needs recalculation.

		// Benchmark: spinner-only updates (Region dirty, but chat Region clean)
		spinnerRegion.region!.dirty = true;
		const spinnerFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

		const start = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			// Simulate commitTextUpdate: mark spinner yoga dirty + region dirty
			spinnerText.children[0]!.textContent = spinnerFrames[i % spinnerFrames.length];
			if (spinnerText.yogaNode) spinnerText.yogaNode.markDirty();
			spinnerRegion.region!.dirty = true;
			renderTree(root, WIDTH);
		}
		const spinnerOnlyTime = performance.now() - start;

		// Benchmark: full tree dirty (simulates no Region optimization)
		const chatRegion = root.children[0]!;
		const startFull = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			spinnerText.children[0]!.textContent = spinnerFrames[i % spinnerFrames.length];
			if (spinnerText.yogaNode) spinnerText.yogaNode.markDirty();
			spinnerRegion.region!.dirty = true;
			chatRegion.region!.dirty = true; // force chat region to re-render too
			renderTree(root, WIDTH);
		}
		const fullTreeTime = performance.now() - startFull;

		const speedup = fullTreeTime / spinnerOnlyTime;

		console.log(`\n  Region Layout Skip (${ITERATIONS} iterations, 500 messages):`);
		console.log(`    Spinner-only (chat cached):  ${spinnerOnlyTime.toFixed(1)}ms`);
		console.log(`    Full tree (all dirty):       ${fullTreeTime.toFixed(1)}ms`);
		console.log(`    Speedup:                     ${speedup.toFixed(1)}x\n`);

		// Region-cached path should be significantly faster
		expect(speedup).toBeGreaterThan(3);
	});

	it('hasOverflowDescendant cache: avoids repeated tree walks', () => {
		const { root } = buildLargeTree(200);
		const WIDTH = 80;

		// Initial render populates the overflow cache for nodes that go through box rendering
		const chatRegion = root.children[0]!;
		chatRegion.region!.dirty = true; // force full render of chat region
		renderTree(root, WIDTH);

		// Benchmark: repeated renders with cache populated
		const start = performance.now();
		for (let i = 0; i < 50; i++) {
			chatRegion.region!.dirty = true;
			renderTree(root, WIDTH);
		}
		const cachedTime = performance.now() - start;

		// Clear all caches and re-render
		function clearCache(node: TwinkiNode): void {
			node._hasOverflow = undefined;
			for (const child of node.children) clearCache(child);
		}

		const startUncached = performance.now();
		for (let i = 0; i < 50; i++) {
			clearCache(chatRegion);
			chatRegion.region!.dirty = true;
			renderTree(root, WIDTH);
		}
		const uncachedTime = performance.now() - startUncached;

		console.log(`\n  Overflow Cache (50 iterations, 200 messages):`);
		console.log(`    With cache:     ${cachedTime.toFixed(1)}ms`);
		console.log(`    Without cache:  ${uncachedTime.toFixed(1)}ms`);
		console.log(`    Speedup:        ${(uncachedTime / cachedTime).toFixed(1)}x\n`);

		// Cached should be faster
		expect(cachedTime).toBeLessThanOrEqual(uncachedTime * 1.1); // allow 10% noise
	});

	it('setWidth triggers layout recalculation (resize scenario still works)', () => {
		const { root, spinnerText, spinnerRegion } = buildLargeTree(50);

		// Initial render at width 80
		const result1 = renderTree(root, 80);
		expect(result1.liveLines.length).toBeGreaterThan(0);

		// Render at different width — should trigger layout (setWidth marks dirty)
		const result2 = renderTree(root, 120);
		expect(result2.liveLines.length).toBeGreaterThan(0);

		// Same width again — isDirty guard should skip layout
		const result3 = renderTree(root, 120);
		expect(result3.liveLines).toEqual(result2.liveLines);
	});

	it('REGRESSION: spinner animation at 10fps stays under 2ms/frame with 1000 messages', () => {
		// This test guards against the bug reported by Joshua Samuel where
		// bun pegged at 98% CPU during shell tool execution because the
		// spinner triggered full yoga layout on 8620 nodes every 100ms.
		const { root, spinnerText, spinnerRegion } = buildLargeTree(1000);
		const WIDTH = 120;
		const FRAMES = 200; // simulate 20 seconds at 10fps

		// Initial render (layout computed once)
		renderTree(root, WIDTH);

		// Simulate 10fps spinner animation (only spinner region dirty)
		const spinnerFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
		const frameTimes: number[] = [];

		for (let i = 0; i < FRAMES; i++) {
			spinnerText.children[0]!.textContent = spinnerFrames[i % spinnerFrames.length];
			if (spinnerText.yogaNode) spinnerText.yogaNode.markDirty();
			spinnerRegion.region!.dirty = true;

			const start = performance.now();
			renderTree(root, WIDTH);
			frameTimes.push(performance.now() - start);
		}

		const avg = frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
		const max = Math.max(...frameTimes);
		const p99 = frameTimes.sort((a, b) => a - b)[Math.floor(frameTimes.length * 0.99)]!;

		console.log(`\n  REGRESSION: Spinner at 10fps, 1000 messages (${FRAMES} frames):`);
		console.log(`    Avg:  ${avg.toFixed(2)}ms/frame`);
		console.log(`    P99:  ${p99.toFixed(2)}ms/frame`);
		console.log(`    Max:  ${max.toFixed(2)}ms/frame`);
		console.log(`    Budget: 100ms/frame (10fps target)\n`);

		// Assert: avg frame time must stay well under the 100ms spinner interval
		// If this regresses, the spinner will cause sustained high CPU
		expect(avg).toBeLessThan(2); // 2ms avg = 2% of 100ms budget
		expect(p99).toBeLessThan(5); // 5ms p99 = 5% of budget
	});
});

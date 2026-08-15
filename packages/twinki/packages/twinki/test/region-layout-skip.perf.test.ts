/**
 * Region layout skip — timing budgets.
 *
 * A spinner that animates at 10fps must not drag a large conversation tree
 * through a full yoga layout on every frame. The numbers below are wall-clock
 * measurements, so they belong in the perf suite, not in the merge gate.
 */
import { describe, it, expect } from 'vitest';
import { renderTree } from '../src/renderer/tree-renderer.js';
import type { TwinkiNode } from '../src/reconciler/types.js';
import { buildLargeTree, SPINNER_FRAMES } from './region-tree-helpers.js';

describe('Region layout skip perf', () => {
	it('isDirty guard: skips calculateLayout when only text content changes in a Region', () => {
		const { root, spinnerText, spinnerRegion } = buildLargeTree(500);
		const WIDTH = 80;
		const ITERATIONS = 100;

		// Initial render — layout is computed
		const result = renderTree(root, WIDTH);
		expect(result.liveLines.length).toBeGreaterThan(0);

		// Benchmark: spinner-only updates (Region dirty, but chat Region clean)
		spinnerRegion.region!.dirty = true;

		const start = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			// Simulate commitTextUpdate: mark spinner yoga dirty + region dirty
			spinnerText.children[0]!.textContent = SPINNER_FRAMES[i % SPINNER_FRAMES.length];
			if (spinnerText.yogaNode) spinnerText.yogaNode.markDirty();
			spinnerRegion.region!.dirty = true;
			renderTree(root, WIDTH);
		}
		const spinnerOnlyTime = performance.now() - start;

		// Benchmark: full tree dirty (simulates no Region optimization)
		const chatRegion = root.children[0]!;
		const startFull = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			spinnerText.children[0]!.textContent = SPINNER_FRAMES[i % SPINNER_FRAMES.length];
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

	it('spinner animation at 10fps stays under 2ms/frame with 1000 messages', () => {
		const { root, spinnerText, spinnerRegion } = buildLargeTree(1000);
		const WIDTH = 120;
		const FRAMES = 200; // simulate 20 seconds at 10fps

		// Initial render (layout computed once)
		renderTree(root, WIDTH);

		// Simulate 10fps spinner animation (only spinner region dirty)
		const frameTimes: number[] = [];

		for (let i = 0; i < FRAMES; i++) {
			spinnerText.children[0]!.textContent = SPINNER_FRAMES[i % SPINNER_FRAMES.length];
			if (spinnerText.yogaNode) spinnerText.yogaNode.markDirty();
			spinnerRegion.region!.dirty = true;

			const start = performance.now();
			renderTree(root, WIDTH);
			frameTimes.push(performance.now() - start);
		}

		const avg = frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
		const max = Math.max(...frameTimes);
		const p99 = frameTimes.sort((a, b) => a - b)[Math.floor(frameTimes.length * 0.99)]!;

		console.log(`\n  Spinner at 10fps, 1000 messages (${FRAMES} frames):`);
		console.log(`    Avg:  ${avg.toFixed(2)}ms/frame`);
		console.log(`    P99:  ${p99.toFixed(2)}ms/frame`);
		console.log(`    Max:  ${max.toFixed(2)}ms/frame`);
		console.log(`    Budget: 100ms/frame (10fps target)\n`);

		// A sustained regression here shows up as high CPU while the spinner runs.
		expect(avg).toBeLessThan(2); // 2ms avg = 2% of 100ms budget
		expect(p99).toBeLessThan(5); // 5ms p99 = 5% of budget
	});
});

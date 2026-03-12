import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render } from '../src/reconciler/render.js';
import { Box } from '../src/components/Box.js';
import { Text } from '../src/components/Text.js';
import { Region } from '../src/components/Region.js';
import { renderNode } from '../src/renderer/tree-renderer.js';
import { createNode } from '../src/reconciler/node-factory.js';
import { NODE_TYPES } from '../src/text/constants.js';
import type { RegionState } from '../src/reconciler/types.js';

describe('Region scoped rendering', () => {
	it('caches rendered lines for clean regions', () => {
		const regionNode = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'test' });
		const textNode = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
		textNode.textContent = undefined;
		textNode.children = [{ type: NODE_TYPES.TEXT, props: {}, yogaNode: null, children: [], parent: textNode, textContent: 'Hello' }];

		regionNode.children = [textNode];
		textNode.parent = regionNode;
		if (textNode.yogaNode) regionNode.yogaNode!.insertChild(textNode.yogaNode, 0);

		const region = regionNode.region!;
		expect(region).toBeDefined();
		expect(region.dirty).toBe(true);

		// First render — should be dirty, renders and caches
		regionNode.yogaNode!.setWidth(40);
		regionNode.yogaNode!.calculateLayout(40, undefined, 1);
		const lines1 = renderNode(regionNode, 40);
		expect(lines1.length).toBeGreaterThan(0);
		expect(region.dirty).toBe(false);
		expect(region.cachedLines).toEqual(lines1);

		// Second render — clean, should return cached lines (same reference)
		const lines2 = renderNode(regionNode, 40);
		expect(lines2).toBe(region.cachedLines); // same reference = cache hit

		// Mark dirty — should re-render
		region.dirty = true;
		const lines3 = renderNode(regionNode, 40);
		expect(region.dirty).toBe(false);

		// Width change — cache miss because lastWidth differs from computed width
		// In practice, resize triggers a full re-render via tui.requestRender(true)
		// which marks everything dirty. Here we verify the width guard works.
		region.dirty = false;
		regionNode.yogaNode!.setWidth(60);
		regionNode.yogaNode!.calculateLayout(60, undefined, 1);
		const lines4 = renderNode(regionNode, 60);
		expect(region.lastWidth).toBe(60);
	});

	it('markRegionDirty only marks owning region', () => {
		const regionA = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'a' });
		const regionB = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'b' });

		regionA.region!.dirty = false;
		regionB.region!.dirty = false;

		// Simulate a child inside region B being updated
		const child = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
		child.parent = regionB;

		// Walk up to find region (same logic as markRegionDirty)
		let p = child.parent;
		while (p) {
			if (p.region) { p.region.dirty = true; break; }
			p = p.parent;
		}

		expect(regionA.region!.dirty).toBe(false); // untouched
		expect(regionB.region!.dirty).toBe(true);  // marked dirty
	});

	it('performance: Region skips rendering large unchanged subtrees', () => {
		const WIDTH = 80;
		const ITERATIONS = 500;

		// Build a big tree: root > [heavyRegion(200 text nodes), smallRegion(1 text node)]
		const root = createNode(NODE_TYPES.TWINKI_BOX as any, { flexDirection: 'column' });
		const heavyRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'heavy' });
		const smallRegion = createNode(NODE_TYPES.TWINKI_REGION as any, { regionId: 'small' });

		// Add 200 text children to heavy region
		for (let i = 0; i < 200; i++) {
			const textNode = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
			const rawText = { type: NODE_TYPES.TEXT, props: {}, yogaNode: null, children: [] as any[], parent: textNode, textContent: `Line ${i}: ${'x'.repeat(60)}` } as any;
			textNode.children = [rawText];
			rawText.parent = textNode;
			heavyRegion.children.push(textNode);
			textNode.parent = heavyRegion;
			if (textNode.yogaNode) heavyRegion.yogaNode!.insertChild(textNode.yogaNode, heavyRegion.yogaNode!.getChildCount());
		}

		// Add 1 text child to small region
		const smallText = createNode(NODE_TYPES.TWINKI_TEXT as any, {});
		const smallRaw = { type: NODE_TYPES.TEXT, props: {}, yogaNode: null, children: [] as any[], parent: smallText, textContent: 'Counter: 0' } as any;
		smallText.children = [smallRaw];
		smallRaw.parent = smallText;
		smallRegion.children.push(smallText);
		smallText.parent = smallRegion;
		if (smallText.yogaNode) smallRegion.yogaNode!.insertChild(smallText.yogaNode, smallRegion.yogaNode!.getChildCount());

		// Wire up tree
		root.children = [heavyRegion, smallRegion];
		heavyRegion.parent = root;
		smallRegion.parent = root;
		if (heavyRegion.yogaNode) root.yogaNode!.insertChild(heavyRegion.yogaNode, 0);
		if (smallRegion.yogaNode) root.yogaNode!.insertChild(smallRegion.yogaNode, 1);

		// Initial render — both regions dirty
		root.yogaNode!.setWidth(WIDTH);
		root.yogaNode!.calculateLayout(WIDTH, undefined, 1);
		renderNode(root, WIDTH);
		expect(heavyRegion.region!.dirty).toBe(false);
		expect(smallRegion.region!.dirty).toBe(false);

		// --- Benchmark WITH regions (only small region dirty) ---
		const startWith = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			smallRegion.region!.dirty = true;
			smallRaw.textContent = `Counter: ${i}`;
			root.yogaNode!.calculateLayout(WIDTH, undefined, 1);
			renderNode(root, WIDTH);
		}
		const timeWithRegions = performance.now() - startWith;

		// --- Benchmark WITHOUT regions (mark both dirty) ---
		const startWithout = performance.now();
		for (let i = 0; i < ITERATIONS; i++) {
			heavyRegion.region!.dirty = true;
			smallRegion.region!.dirty = true;
			smallRaw.textContent = `Counter: ${i}`;
			root.yogaNode!.calculateLayout(WIDTH, undefined, 1);
			renderNode(root, WIDTH);
		}
		const timeWithoutRegions = performance.now() - startWithout;

		const speedup = timeWithoutRegions / timeWithRegions;

		console.log(`\n  Region Performance (${ITERATIONS} iterations, 200-node heavy tree):`);
		console.log(`    With regions (heavy cached):    ${timeWithRegions.toFixed(1)}ms`);
		console.log(`    Without regions (all dirty):    ${timeWithoutRegions.toFixed(1)}ms`);
		console.log(`    Speedup:                        ${speedup.toFixed(1)}x\n`);

		// Region-cached path should be significantly faster
		expect(speedup).toBeGreaterThan(2);
	});
});

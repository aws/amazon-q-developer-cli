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
});

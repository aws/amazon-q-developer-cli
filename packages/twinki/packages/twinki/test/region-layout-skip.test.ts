import { describe, it, expect } from 'vitest';
import { renderTree } from '../src/renderer/tree-renderer.js';
import { buildLargeTree } from './region-tree-helpers.js';

describe('Region layout skip optimization', () => {
	it('setWidth triggers layout recalculation (resize scenario still works)', () => {
		const { root } = buildLargeTree(50);

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
});

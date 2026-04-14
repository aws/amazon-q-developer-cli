import { describe, it, expect, vi } from 'vitest';
import { measureElement } from '../src/index.js';

describe('measureElement', () => {
	it('returns 0x0 for null node', () => {
		expect(measureElement(null)).toEqual({ width: 0, height: 0 });
	});

	it('returns 0x0 for node without yogaNode', () => {
		expect(measureElement({})).toEqual({ width: 0, height: 0 });
	});

	it('returns computed dimensions from yogaNode', () => {
		const node = {
			yogaNode: {
				getComputedWidth: () => 80,
				getComputedHeight: () => 24,
			},
			rootContainer: {
				yogaNode: {
					isDirty: () => false,
					setWidth: vi.fn(),
					calculateLayout: vi.fn(),
				},
			},
		};
		expect(measureElement(node)).toEqual({ width: 80, height: 24 });
	});

	it('calls calculateLayout when yoga tree is dirty', () => {
		const calculateLayout = vi.fn();
		const setWidth = vi.fn();
		const node = {
			yogaNode: {
				getComputedWidth: () => 120,
				getComputedHeight: () => 40,
			},
			rootContainer: {
				yogaNode: {
					isDirty: () => true,
					setWidth,
					calculateLayout,
				},
			},
		};
		measureElement(node);
		expect(setWidth).toHaveBeenCalled();
		expect(calculateLayout).toHaveBeenCalled();
	});

	it('skips calculateLayout when yoga tree is NOT dirty', () => {
		const calculateLayout = vi.fn();
		const setWidth = vi.fn();
		const node = {
			yogaNode: {
				getComputedWidth: () => 80,
				getComputedHeight: () => 24,
			},
			rootContainer: {
				yogaNode: {
					isDirty: () => false,
					setWidth,
					calculateLayout,
				},
			},
		};
		measureElement(node);
		expect(setWidth).not.toHaveBeenCalled();
		expect(calculateLayout).not.toHaveBeenCalled();
	});

	it('works without rootContainer', () => {
		const node = {
			yogaNode: {
				getComputedWidth: () => 50,
				getComputedHeight: () => 10,
			},
		};
		expect(measureElement(node)).toEqual({ width: 50, height: 10 });
	});
});

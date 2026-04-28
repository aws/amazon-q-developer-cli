/**
 * Resize render count tests.
 *
 * Verifies that:
 * 1. Render count stays bounded during rapid resize (no oscillation loop)
 * 2. Scrollbar-style oscillation (alternating widths) doesn't spiral
 * 3. Renders settle after resize stops
 */
import { describe, it, expect } from 'vitest';
import { TestTerminal, MutableComponent, wait } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';

describe('Resize render count', () => {
	it('rapid resize: render count stays bounded', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term, {});
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		comp.lines = ['Line 1', 'Line 2', 'Line 3'];
		tui.requestRender();
		await wait(50);

		const before = tui.perfRenderCount;

		// 20 rapid width changes
		for (let i = 0; i < 20; i++) {
			term.resize(80 + i, 24);
		}
		await wait(200);

		const renders = tui.perfRenderCount - before;

		// Each resize triggers at most 1 force render via nextTick.
		// Must be bounded — no infinite cascade.
		expect(renders).toBeLessThanOrEqual(25);
		expect(renders).toBeGreaterThan(0);

		tui.stop();
	});

	it('scrollbar oscillation pattern: no spiral', async () => {
		const term = new TestTerminal(140, 24);
		const tui = new TUI(term, {});
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		comp.lines = Array.from({ length: 30 }, (_, i) => `Line ${i}: ${'x'.repeat(100)}`);
		tui.requestRender();
		await wait(50);

		const before = tui.perfRenderCount;

		// Simulate scrollbar oscillation: 140 → 138 → 140 → 138 ...
		for (let i = 0; i < 10; i++) {
			term.resize(i % 2 === 0 ? 138 : 140, 24);
			await wait(5);
		}
		await wait(200);

		const renders = tui.perfRenderCount - before;

		// 10 resize steps → bounded renders, no infinite loop
		expect(renders).toBeLessThanOrEqual(25);
		expect(renders).toBeGreaterThan(0);

		tui.stop();
	});

	it('resize settles: no renders after resize stops', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term, {});
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		comp.lines = ['Hello'];
		tui.requestRender();
		await wait(50);

		term.resize(100, 24);
		await wait(200);

		// Measure renders over a quiet period
		const before = tui.perfRenderCount;
		await wait(200);
		const after = tui.perfRenderCount;

		expect(after - before).toBe(0);

		tui.stop();
	});
});

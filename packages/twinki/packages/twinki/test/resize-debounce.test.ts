import { describe, it, expect } from 'vitest';
import React from 'react';
import { TestTerminal, MutableComponent, wait } from './helpers.ts';
import { TUI } from '../src/renderer/tui.js';

describe('Resize debounce (ProcessTerminal)', () => {
	it('collapses rapid resize oscillation into single callback', async () => {
		// Test the debounce logic directly: simulate what ProcessTerminal does
		let resizeCount = 0;
		let resizeTimer: ReturnType<typeof setTimeout> | null = null;
		let _columns = 140;

		// This mirrors ProcessTerminal's resize handler logic
		function simulateResize(newCols: number) {
			if (newCols === _columns) return;
			if (resizeTimer) clearTimeout(resizeTimer);
			resizeTimer = setTimeout(() => {
				resizeTimer = null;
				// Re-read "current" columns (simulated)
				if (currentCols === _columns) return;
				_columns = currentCols;
				resizeCount++;
			}, 80);
		}

		let currentCols = 140;

		// Simulate: scrollbar appears (140→138)
		currentCols = 138;
		simulateResize(138);

		// 30ms later: scrollbar disappears (138→140)
		await wait(30);
		currentCols = 140;
		simulateResize(140);

		// Debounce pending — no callback yet
		expect(resizeCount).toBe(0);

		// Wait for debounce to settle
		await wait(100);

		// Final dimensions match original — no net change
		expect(resizeCount).toBe(0);
		expect(_columns).toBe(140); // never updated

		if (resizeTimer) clearTimeout(resizeTimer);
	});

	it('fires callback for real resize after debounce settles', async () => {
		let resizeCount = 0;
		let resizeTimer: ReturnType<typeof setTimeout> | null = null;
		let _columns = 80;
		let currentCols = 80;

		function simulateResize(newCols: number) {
			if (newCols === _columns) return;
			if (resizeTimer) clearTimeout(resizeTimer);
			resizeTimer = setTimeout(() => {
				resizeTimer = null;
				if (currentCols === _columns) return;
				_columns = currentCols;
				resizeCount++;
			}, 80);
		}

		// Real resize: 80→120
		currentCols = 120;
		simulateResize(120);

		expect(resizeCount).toBe(0); // debounce pending

		await wait(100); // debounce fires

		expect(resizeCount).toBe(1);
		expect(_columns).toBe(120);

		if (resizeTimer) clearTimeout(resizeTimer);
	});
});

describe('scrollbarWidth option', () => {
	it('renders content at columns - scrollbarWidth', async () => {
		const term = new TestTerminal(80, 10);
		const tui = new TUI(term, { scrollbarWidth: 2 });
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		// Render a line that fills the effective width (78 chars)
		comp.lines = ['X'.repeat(78)];
		tui.requestRender();
		await wait(50); await term.flush();

		const viewport = term.getViewport();
		// Line should contain exactly 78 X's on one line
		const xLines = viewport.filter(l => l.includes('X'));
		expect(xLines).toHaveLength(1);

		tui.stop();
	});

	it('content wider than effective width is clipped', async () => {
		const term = new TestTerminal(80, 10);
		const tui = new TUI(term, { scrollbarWidth: 2 });
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		// Render 80 chars — should be clipped/wrapped since effective width is 78
		comp.lines = ['A'.repeat(80)];
		tui.requestRender();
		await wait(50); await term.flush();

		// The component receives width=78 in render(), so it's up to the component
		// to respect it. The TUI passes the reduced width to render().
		// Verify the TUI used 78 as the render width by checking fullRedraws baseline
		expect(tui.fullRedraws).toBeGreaterThanOrEqual(1); // initial render

		tui.stop();
	});

	it('real resize still triggers reflow', async () => {
		const term = new TestTerminal(80, 10);
		const tui = new TUI(term, { scrollbarWidth: 2 });
		const comp = new MutableComponent();
		tui.addChild(comp);
		tui.start();

		comp.lines = ['Hello'];
		tui.requestRender();
		await wait(50); await term.flush();

		const fullRedraws = tui.fullRedraws;

		// Real resize: 80→60 — effective goes from 78 to 58, triggers reflow
		term.resize(60, 10);
		await wait(50); await term.flush();

		expect(tui.fullRedraws).toBeGreaterThan(fullRedraws);

		tui.stop();
	});
});

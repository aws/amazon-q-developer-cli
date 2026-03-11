import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import {
	TestTerminal, MutableComponent, analyzeFlicker,
	wait, renderAndCapture, type Frame,
} from './helpers.js';

describe('Flicker Detection', () => {
	it('spinner animation produces zero flicker', async () => {
		const spinners = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
		const term = new TestTerminal(40, 3);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [`${spinners[0]} Loading...`];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 1; i < 10; i++) {
			comp.lines = [`${spinners[i]!} Loading...`];
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBe(10);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);
		expect(report.events.length).toBe(0);

		tui.stop();
	});

	it('streaming text append produces zero flicker', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 2; i <= 8; i++) {
			comp.lines = [...comp.lines, `Line ${i}`];
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBe(8);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('content replacement produces zero flicker', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Header', 'Content A', 'Footer'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 0; i < 10; i++) {
			comp.lines = ['Header', `Content ${String.fromCharCode(65 + i)}`, 'Footer'];
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('detects injected flicker (validates the detector)', () => {
		// Row 0 flickers: "XY" → "X " → "XY" (col 1 goes Y→space→Y)
		// Content height stays constant (1 non-blank line in each frame)
		const frames: Frame[] = [
			{ index: 0, timestamp: 0n, viewport: ['XY'], writeBytes: 10, isFull: false },
			{ index: 1, timestamp: 1n, viewport: ['X '], writeBytes: 10, isFull: false },
			{ index: 2, timestamp: 2n, viewport: ['XY'], writeBytes: 10, isFull: false },
		];

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(false);
		expect(report.events.length).toBeGreaterThan(0);
		expect(report.events[0]!.frameIndex).toBe(1);
		expect(report.events[0]!.row).toBe(0);
		expect(report.events[0]!.col).toBe(1);
	});

	it('rapid state changes coalesce into single render', async () => {
		const term = new TestTerminal(40, 3);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['initial'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Fire 5 rapid requestRender calls before nextTick fires
		comp.lines = ['change 1'];
		tui.requestRender();
		comp.lines = ['change 2'];
		tui.requestRender();
		comp.lines = ['change 3'];
		tui.requestRender();
		comp.lines = ['change 4'];
		tui.requestRender();
		comp.lines = ['final'];
		tui.requestRender();

		await wait();
		await term.flush();

		const frames = term.getFrames();
		// Should be 2 frames: initial + one coalesced update
		expect(frames.length).toBe(2);
		expect(frames[1]!.viewport[0]).toContain('final');

		tui.stop();
	});
});

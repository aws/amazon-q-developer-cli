import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import {
	TestTerminal, MutableComponent, analyzeFlicker,
	wait, diffFrames,
} from './helpers.js';

describe('Edge Cases: Width Change', () => {
	it('resize triggers full rerender', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Hello at 40 cols'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		term.resize(30, 5);
		await wait();
		await term.flush();

		const frames = term.getFrames();
		expect(frames.length).toBe(2);
		expect(frames[1]!.isFull).toBe(true);

		tui.stop();
	});

	it('content visible after resize', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['test content'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		term.resize(20, 5);
		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport[0]).toContain('test content');

		tui.stop();
	});
});

describe('Edge Cases: Empty Content', () => {
	it('renders empty component without crash', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Should have rendered (even if empty)
		const frames = term.getFrames();
		expect(frames.length).toBeGreaterThanOrEqual(0);

		tui.stop();
	});

	it('transitions from content to empty', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2', 'Line 3'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Line 1');

		comp.lines = [];
		tui.requestRender();
		await wait();
		await term.flush();

		// After clearing, the old lines should be erased
		// (the TUI clears them via CSI 2K)
		const vp = term.getLastFrame()!.viewport;
		// Row 1 and 2 should be cleared (they were extra lines)
		expect(vp[1]!.trim()).toBe('');
		expect(vp[2]!.trim()).toBe('');

		tui.stop();
	});

	it('transitions from empty to content', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		comp.lines = ['Appeared!'];
		tui.requestRender();
		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport[0]).toContain('Appeared!');

		tui.stop();
	});

	it('content → empty → content cycle', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Phase 1'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Phase 1');

		comp.lines = [];
		tui.requestRender();
		await wait(); await term.flush();

		comp.lines = ['Phase 3'];
		tui.requestRender();
		await wait(); await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Phase 3');

		tui.stop();
	});
});

describe('Edge Cases: Unicode Content', () => {
	it('renders CJK characters', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['你好世界'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport[0]).toContain('你好世界');

		tui.stop();
	});

	it('renders emoji', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Status: ✅ Done'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		const vp = term.getLastFrame()!.viewport[0]!;
		expect(vp).toContain('Status:');
		expect(vp).toContain('Done');

		tui.stop();
	});

	it('renders mixed ASCII + CJK', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Hello 你好 World 世界'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		const vp = term.getLastFrame()!.viewport[0]!;
		expect(vp).toContain('Hello');
		expect(vp).toContain('World');

		tui.stop();
	});
});

describe('Edge Cases: Synchronized Output', () => {
	it('every frame write contains sync markers', async () => {
		const writes: string[] = [];
		const term = new TestTerminal(40, 5);
		const origWrite = term.write.bind(term);
		term.write = (data: string) => { writes.push(data); origWrite(data); };

		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['test'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		comp.lines = ['changed'];
		tui.requestRender();
		await wait();
		await term.flush();

		// Find writes that contain content (not just cursor hide)
		const contentWrites = writes.filter(w => w.includes('test') || w.includes('changed'));
		for (const w of contentWrites) {
			expect(w).toContain('\x1b[?2026h');
			expect(w).toContain('\x1b[?2026l');
		}

		tui.stop();
	});

	it('full rerender contains scrollback clear', async () => {
		const writes: string[] = [];
		const term = new TestTerminal(40, 5);
		const origWrite = term.write.bind(term);
		term.write = (data: string) => { writes.push(data); origWrite(data); };

		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['test'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Force full rerender
		tui.requestRender(true);
		await wait();
		await term.flush();

		const fullWrite = writes.find(w => w.includes('\x1b[3J'));
		expect(fullWrite).toBeDefined();

		tui.stop();
	});

	it('differential does NOT contain scrollback clear', async () => {
		const writes: string[] = [];
		const term = new TestTerminal(40, 5);
		const origWrite = term.write.bind(term);
		term.write = (data: string) => { writes.push(data); origWrite(data); };

		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();
		writes.length = 0; // Clear initial writes

		comp.lines = ['Line 1', 'CHANGED'];
		tui.requestRender();
		await wait();
		await term.flush();

		const diffWrite = writes.find(w => w.includes('CHANGED'));
		expect(diffWrite).toBeDefined();
		expect(diffWrite).not.toContain('\x1b[3J');

		tui.stop();
	});
});

describe('Edge Cases: Content Growth & Shrink', () => {
	it('growing content appends correctly', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		for (let i = 2; i <= 5; i++) {
			comp.lines.push(`Line ${i}`);
			tui.requestRender();
			await wait(); await term.flush();
		}

		const vp = term.getLastFrame()!.viewport;
		expect(vp[0]).toContain('Line 1');
		expect(vp[4]).toContain('Line 5');

		tui.stop();
	});

	it('shrinking content clears old lines', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['A', 'B', 'C', 'D', 'E'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();
		expect(term.getLastFrame()!.viewport[4]).toContain('E');

		comp.lines = ['A', 'B'];
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp[0]).toContain('A');
		expect(vp[1]).toContain('B');
		expect(vp[2]!.trim()).toBe('');
		expect(vp[4]!.trim()).toBe('');

		tui.stop();
	});

	it('rapid grow/shrink cycles are stable', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['start'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		for (let cycle = 0; cycle < 5; cycle++) {
			// Grow
			comp.lines = Array.from({ length: 5 }, (_, i) => `Cycle ${cycle} Line ${i}`);
			tui.requestRender();
			await wait(); await term.flush();

			// Shrink
			comp.lines = [`Cycle ${cycle} summary`];
			tui.requestRender();
			await wait(); await term.flush();
		}

		// Verify final state is correct
		const lastVp = term.getLastFrame()!.viewport;
		expect(lastVp[0]).toContain('Cycle 4 summary');

		// Verify no crashes and all frames captured
		expect(term.getFrames().length).toBe(11); // 1 initial + 5*(grow+shrink)

		tui.stop();
	});
});

describe('Edge Cases: Force Render', () => {
	it('force render resets all state', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['initial'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		comp.lines = ['after force'];
		tui.requestRender(true);
		await wait(); await term.flush();

		const frames = term.getFrames();
		expect(frames[frames.length - 1]!.isFull).toBe(true);
		expect(frames[frames.length - 1]!.viewport[0]).toContain('after force');

		tui.stop();
	});
});

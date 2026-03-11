/**
 * Stress test: large output with rapid scrolling.
 *
 * Simulates a real-world scenario like streaming a long AI response
 * or tailing a log file — content grows beyond the viewport and
 * the terminal must scroll while maintaining zero flicker.
 */
import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import {
	TestTerminal, MutableComponent, analyzeFlicker,
	wait, diffFrames,
} from './helpers.js';

describe('Large output + scrolling', () => {
	it('50 lines into 10-row viewport: latest lines visible', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Append 50 lines one at a time
		for (let i = 1; i <= 50; i++) {
			comp.lines = [...comp.lines, `Line ${i}: ${'x'.repeat(40)}`];
			tui.requestRender();
			await wait(); await term.flush();
		}

		const lastFrame = term.getLastFrame()!;
		// Latest line should be visible
		expect(lastFrame.viewport.some(l => l.includes('Line 50'))).toBe(true);
		// Early lines scrolled out of viewport
		expect(lastFrame.viewport.some(l => l.includes('Line 1:'))).toBe(false);

		tui.stop();
	});

	it('rapid append: zero flicker across 100 frames', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['=== Log Output ==='];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		for (let i = 1; i <= 100; i++) {
			comp.lines.push(`[${String(i).padStart(3, '0')}] Event: data received`);
			tui.requestRender();
			await wait(5); await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBeGreaterThan(50);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('differential updates stay small even with large content', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		// Start with 30 lines (3x viewport)
		comp.lines = Array.from({ length: 30 }, (_, i) => `Line ${i + 1}: content here`);
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();
		const firstFrame = term.getLastFrame()!;

		// Append one line
		comp.lines.push('Line 31: NEW');
		tui.requestRender();
		await wait(); await term.flush();
		const secondFrame = term.getLastFrame()!;

		// Differential should be much smaller than first render
		expect(secondFrame.isFull).toBe(false);
		expect(secondFrame.writeBytes).toBeLessThan(firstFrame.writeBytes);

		tui.stop();
	});

	it('content replacement in large buffer: only changed rows in diff', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = Array.from({ length: 10 }, (_, i) => `Row ${i}: static content`);
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Change only row 5
		comp.lines[5] = 'Row 5: CHANGED';
		tui.requestRender();
		await wait(); await term.flush();

		const frames = term.getFrames();
		const diff = diffFrames(frames[0]!, frames[1]!);
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('row 5');

		tui.stop();
	});

	it('simulated streaming: word-by-word append to last line', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Header', ''];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Stream words into the last line
		const words = 'The quick brown fox jumps over the lazy dog'.split(' ');
		let current = '';
		for (const word of words) {
			current += (current ? ' ' : '') + word;
			comp.lines[1] = current;
			tui.requestRender();
			await wait(5); await term.flush();
		}

		const lastFrame = term.getLastFrame()!;
		expect(lastFrame.viewport.some(l => l.includes('lazy dog'))).toBe(true);

		// Header should never change
		const frames = term.getFrames();
		for (let i = 1; i < frames.length; i++) {
			const diff = diffFrames(frames[i - 1]!, frames[i]!);
			expect(diff.every(d => !d.includes('Header'))).toBe(true);
		}

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('grow beyond viewport then shrink back: stable rendering', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Start'];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Grow to 30 lines
		for (let i = 2; i <= 30; i++) {
			comp.lines.push(`Line ${i}`);
			tui.requestRender();
			await wait(5); await term.flush();
		}

		expect(term.getLastFrame()!.viewport.some(l => l.includes('Line 30'))).toBe(true);

		// Shrink back to 5 lines
		comp.lines = ['Summary', 'Line A', 'Line B', 'Line C', 'Done'];
		tui.requestRender();
		await wait(); await term.flush();

		expect(term.getLastFrame()!.viewport.some(l => l.includes('Summary'))).toBe(true);
		expect(term.getLastFrame()!.viewport.some(l => l.includes('Done'))).toBe(true);

		tui.stop();
	});

	it('mixed static header + scrolling body + static footer', async () => {
		const term = new TestTerminal(60, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();

		function buildLines(bodyLines: string[]): string[] {
			return [
				'=== HEADER ===',
				...bodyLines,
				'=== FOOTER ===',
			];
		}

		comp.lines = buildLines(['Body line 1']);
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Append body lines, keeping header/footer
		const bodyLines: string[] = ['Body line 1'];
		for (let i = 2; i <= 20; i++) {
			bodyLines.push(`Body line ${i}`);
			comp.lines = buildLines(bodyLines);
			tui.requestRender();
			await wait(5); await term.flush();
		}

		const lastFrame = term.getLastFrame()!;
		// Footer should be visible (it's the last line)
		expect(lastFrame.viewport.some(l => l.includes('FOOTER'))).toBe(true);
		// Latest body line should be visible
		expect(lastFrame.viewport.some(l => l.includes('Body line 20'))).toBe(true);

		tui.stop();
	});

	it('burst: 200 lines in rapid succession', async () => {
		const term = new TestTerminal(80, 15);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [];
		tui.addChild(comp);
		tui.start();

		await wait(); await term.flush();

		// Burst 200 lines
		for (let i = 0; i < 200; i++) {
			comp.lines.push(`[${new Date().toISOString()}] Event ${i}: payload=${'a'.repeat(30)}`);
		}
		tui.requestRender();
		await wait(); await term.flush();

		const lastFrame = term.getLastFrame()!;
		expect(lastFrame.viewport.some(l => l.includes('Event 199'))).toBe(true);

		// Should only be 2 frames (empty + burst), not 200
		expect(term.getFrames().length).toBe(2);

		tui.stop();
	});
});

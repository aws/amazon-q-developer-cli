/**
 * Component integration tests using frame capture.
 *
 * Every test captures frames, verifies transitions, checks differential
 * rendering, and uses the screenshot/diff infrastructure.
 */
import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import type { Terminal } from '../src/terminal/terminal.js';
import type { Frame } from './helpers.js';
import { analyzeFlicker, diffFrames, serializeFrame } from './helpers.js';

// --- Frame-capturing terminal for React tests ---

class ReactTestTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private _cols: number;
	private _rows: number;
	private frames: Frame[] = [];
	private frameIndex = 0;
	private pendingCapture = false;
	private pendingBytes = 0;
	private pendingIsFull = false;

	constructor(cols = 60, rows = 10) {
		this._cols = cols; this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	}
	get kittyProtocolActive() { return true; }
	get columns() { return this._cols; }
	get rows() { return this._rows; }
	start(onInput: (data: string) => void) { this.inputHandler = onInput; }
	stop() {}
	async drainInput() {}
	write(data: string) {
		this.xterm.write(data);
		if (data.includes('\x1b[?2026l')) {
			this.pendingCapture = true;
			this.pendingBytes = data.length;
			this.pendingIsFull = data.includes('\x1b[3J');
		}
	}
	sendInput(data: string) { this.inputHandler?.(data); }
	moveBy(n: number) { if (n > 0) this.write(`\x1b[${n}B`); else if (n < 0) this.write(`\x1b[${-n}A`); }
	hideCursor() { this.write('\x1b[?25l'); }
	showCursor() { this.write('\x1b[?25h'); }
	clearLine() { this.write('\x1b[K'); }
	clearFromCursor() { this.write('\x1b[J'); }
	clearScreen() { this.write('\x1b[2J\x1b[H'); }
	setTitle() {}
	enableMouse() {}
	disableMouse() {}

	async flush(): Promise<void> {
		await new Promise<void>(resolve => this.xterm.write('', resolve));
		if (this.pendingCapture) {
			const buf = this.xterm.buffer.active;
			const viewport: string[] = [];
			for (let i = 0; i < this._rows; i++) {
				const line = buf.getLine(buf.viewportY + i);
				viewport.push(line ? line.translateToString(true) : '');
			}
			this.frames.push({
				index: this.frameIndex++,
				timestamp: process.hrtime.bigint(),
				viewport,
				writeBytes: this.pendingBytes,
				isFull: this.pendingIsFull,
			});
			this.pendingCapture = false;
			this.pendingBytes = 0;
			this.pendingIsFull = false;
		}
	}

	getFrames(): Frame[] { return [...this.frames]; }
	getLastFrame(): Frame | undefined { return this.frames[this.frames.length - 1]; }
}

async function wait(ms = 20) { await new Promise(r => setTimeout(r, ms)); }

// ============================================================
// Text Component — Frame Capture Tests
// ============================================================

describe('Text component (frame capture)', () => {
	it('plain text: first frame captures content', async () => {
		const term = new ReactTestTerminal();
		const inst = render(React.createElement(Text, null, 'hello world'), { terminal: term, exitOnCtrlC: false });

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.index).toBe(0);
		expect(frame.viewport.some(l => l.includes('hello world'))).toBe(true);

		// Screenshot
		const screenshot = serializeFrame(frame, 60);
		expect(screenshot).toContain('hello world');
		expect(screenshot).toContain('Frame 0');

		inst.unmount();
	});

	it('nested <Text>: inner content visible in frame', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Text, null, 'Value: ', React.createElement(Text, { bold: true }, '42')),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport.some(l => l.includes('Value: 42'))).toBe(true);

		inst.unmount();
	});

	it('deeply nested Text: all levels render', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Text, null,
				'a ',
				React.createElement(Text, null, 'b ', React.createElement(Text, null, 'c')),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();
		expect(term.getLastFrame()!.viewport.some(l => l.includes('a b c'))).toBe(true);

		inst.unmount();
	});

	it('counter pattern: label + styled value in single frame', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Text, null,
				'  Value: ',
				React.createElement(Text, { color: 'yellow', bold: true }, '0'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport.some(l => l.includes('Value: 0'))).toBe(true);

		inst.unmount();
	});
});

// ============================================================
// Box Layout — Frame Capture Tests
// ============================================================

describe('Box with border (frame capture)', () => {
	it('bordered Box with Text child: text visible inside border', async () => {
		const term = new ReactTestTerminal(40, 10);
		const inst = render(
			React.createElement(Box, { borderStyle: 'round', padding: 1 },
				React.createElement(Text, null, 'Hello'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		// The border should be present
		expect(frame.viewport.some(l => l.includes('╭'))).toBe(true);
		// The text should be visible inside the border (after border + padding)
		const textLine = frame.viewport.find(l => l.includes('Hello'))!;
		expect(textLine).toBeDefined();
		const idx = textLine.indexOf('Hello');
		expect(idx).toBeGreaterThan(0); // After border char

		inst.unmount();
	});
});

describe('Box layout (frame capture)', () => {
	it('column layout: children on consecutive rows in frame', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'AAA'),
				React.createElement(Text, null, 'BBB'),
				React.createElement(Text, null, 'CCC'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		const a = frame.viewport.findIndex(l => l.includes('AAA'));
		const b = frame.viewport.findIndex(l => l.includes('BBB'));
		const c = frame.viewport.findIndex(l => l.includes('CCC'));
		expect(a).toBeGreaterThanOrEqual(0);
		expect(b).toBe(a + 1);
		expect(c).toBe(b + 1);

		inst.unmount();
	});
});

// ============================================================
// Rerender — Frame Transitions
// ============================================================

describe('Rerender transitions (frame capture)', () => {
	it('rerender produces a second frame with updated content', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Text, null, 'Version 1'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		inst.rerender(React.createElement(Text, null, 'Version 2'));
		await wait(); await term.flush();

		const frames = term.getFrames();
		expect(frames.length).toBe(2);

		// Frame 0: Version 1
		expect(frames[0]!.viewport.some(l => l.includes('Version 1'))).toBe(true);
		// Frame 1: Version 2
		expect(frames[1]!.viewport.some(l => l.includes('Version 2'))).toBe(true);

		// Diff shows what changed
		const diff = diffFrames(frames[0]!, frames[1]!);
		expect(diff.length).toBeGreaterThan(0);
		expect(diff.some(d => d.includes('Version 1') && d.includes('Version 2'))).toBe(true);

		// Second frame is differential (not full clear)
		expect(frames[1]!.isFull).toBe(false);

		inst.unmount();
	});

	it('multiple rerenders: zero flicker', async () => {
		const term = new ReactTestTerminal();
		const inst = render(
			React.createElement(Text, null, 'v0'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		for (let i = 1; i <= 10; i++) {
			inst.rerender(React.createElement(Text, null, `v${i}`));
			await wait(); await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBe(11);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		inst.unmount();
	});

	it('content change in Box: only changed row in diff', async () => {
		const term = new ReactTestTerminal();

		function App({ body }: { body: string }) {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'Header'),
				React.createElement(Text, null, body),
				React.createElement(Text, null, 'Footer'),
			);
		}

		const inst = render(
			React.createElement(App, { body: 'Body v1' }),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(); await term.flush();

		inst.rerender(React.createElement(App, { body: 'Body v2' }));
		await wait(); await term.flush();

		const frames = term.getFrames();
		const diff = diffFrames(frames[0]!, frames[1]!);

		// Only the body row changed
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('Body v1');
		expect(diff[0]).toContain('Body v2');

		// Header and Footer untouched
		expect(frames[1]!.viewport.some(l => l.includes('Header'))).toBe(true);
		expect(frames[1]!.viewport.some(l => l.includes('Footer'))).toBe(true);

		inst.unmount();
	});
});

// ============================================================
// Hooks — Frame Capture Tests
// ============================================================

describe('Hooks in render tree (frame capture)', () => {
	it('useApp and useInput work — no context error', async () => {
		const { useApp } = await import('../src/hooks/useApp.js');
		const { useInput } = await import('../src/hooks/useInput.js');

		function HookApp() {
			const { exit } = useApp();
			useInput(() => {});
			return React.createElement(Text, null, 'hooks ok');
		}

		const term = new ReactTestTerminal();
		const inst = render(React.createElement(HookApp), { terminal: term, exitOnCtrlC: false });

		await wait(); await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport.some(l => l.includes('hooks ok'))).toBe(true);

		inst.unmount();
	});
});

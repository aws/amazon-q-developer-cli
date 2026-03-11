import { describe, it, expect } from 'vitest';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import type { Terminal } from '../src/terminal/terminal.js';

// --- Inline FrameCapturingTerminal (avoids cross-package import issues) ---

interface Frame {
	index: number;
	timestamp: bigint;
	viewport: string[];
	writeBytes: number;
	isFull: boolean;
}

class FrameCapturingTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private resizeHandler?: () => void;
	private _cols: number;
	private _rows: number;
	private frames: Frame[] = [];
	private frameIndex = 0;
	private pendingCapture = false;
	private pendingBytes = 0;
	private pendingIsFull = false;

	constructor(cols = 40, rows = 10) {
		this._cols = cols;
		this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	}

	get kittyProtocolActive() { return true; }
	get columns() { return this._cols; }
	get rows() { return this._rows; }
	start(onInput: (data: string) => void, onResize: () => void) {
		this.inputHandler = onInput;
		this.resizeHandler = onResize;
	}
	stop() {}
	async drainInput() {}

	write(data: string) {
		this.xterm.write(data);
		if (data.includes('\x1b[?2026l')) {
			this.pendingCapture = true;
			this.pendingBytes = data.length;
			this.pendingIsFull = data.includes('\x1b[3J') || data.includes('\x1b[2J');
		}
	}

	sendInput(data: string) { this.inputHandler?.(data); }
	resize(cols: number, rows: number) {
		this._cols = cols;
		this._rows = rows;
		this.xterm.resize(cols, rows);
		this.resizeHandler?.();
	}
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
			this.frames.push({
				index: this.frameIndex++,
				timestamp: process.hrtime.bigint(),
				viewport: this.getViewport(),
				writeBytes: this.pendingBytes,
				isFull: this.pendingIsFull,
			});
			this.pendingCapture = false;
			this.pendingBytes = 0;
			this.pendingIsFull = false;
		}
	}

	getViewport(): string[] {
		const buf = this.xterm.buffer.active;
		const lines: string[] = [];
		for (let i = 0; i < this._rows; i++) {
			const line = buf.getLine(buf.viewportY + i);
			lines.push(line ? line.translateToString(true) : '');
		}
		return lines;
	}

	getFrames(): Frame[] { return [...this.frames]; }
	getLastFrame(): Frame | undefined { return this.frames[this.frames.length - 1]; }
}

// --- Helpers ---

class MutableComponent implements Component {
	lines: string[] = [];
	render() { return this.lines; }
	invalidate() {}
}

function serializeFrame(frame: Frame): string {
	const width = 40;
	const header = `Frame ${frame.index} (${frame.writeBytes}B, ${frame.isFull ? 'full' : 'diff'}):`;
	const top = '┌' + '─'.repeat(width) + '┐';
	const bottom = '└' + '─'.repeat(width) + '┘';
	const lines = frame.viewport.map(l => '│' + l.padEnd(width) + '│');
	return [header, top, ...lines, bottom].join('\n');
}

function diffFrames(a: Frame, b: Frame): string[] {
	const changed: string[] = [];
	const max = Math.max(a.viewport.length, b.viewport.length);
	for (let i = 0; i < max; i++) {
		if ((a.viewport[i] ?? '') !== (b.viewport[i] ?? '')) {
			changed.push(`row ${i}: ${JSON.stringify(a.viewport[i])} → ${JSON.stringify(b.viewport[i])}`);
		}
	}
	return changed;
}

async function wait(ms = 15) {
	await new Promise(r => setTimeout(r, ms));
}

// --- Tests ---

describe('Frame Capture & Transitions', () => {
	it('captures first render as full frame', async () => {
		const term = new FrameCapturingTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Hello World'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		const frames = term.getFrames();
		expect(frames.length).toBe(1);
		expect(frames[0]!.index).toBe(0);
		expect(frames[0]!.viewport[0]).toContain('Hello World');
		// First render is NOT a full clear (no scrollback clear)
		expect(frames[0]!.isFull).toBe(false);

		tui.stop();
	});

	it('captures differential update as non-full frame', async () => {
		const term = new FrameCapturingTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Change one line
		comp.lines = ['Line 1', 'CHANGED'];
		tui.requestRender();

		await wait();
		await term.flush();

		const frames = term.getFrames();
		expect(frames.length).toBe(2);
		expect(frames[1]!.isFull).toBe(false);
		expect(frames[1]!.viewport[1]).toContain('CHANGED');

		tui.stop();
	});

	it('captures width change as full frame', async () => {
		const term = new FrameCapturingTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['test'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Resize triggers full rerender
		term.resize(30, 5);

		await wait();
		await term.flush();

		const frames = term.getFrames();
		expect(frames.length).toBe(2);
		expect(frames[1]!.isFull).toBe(true);

		tui.stop();
	});

	it('diffFrames shows what changed between frames', async () => {
		const term = new FrameCapturingTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Header', 'Body v1', 'Footer'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		comp.lines = ['Header', 'Body v2', 'Footer'];
		tui.requestRender();

		await wait();
		await term.flush();

		const frames = term.getFrames();
		const diff = diffFrames(frames[0]!, frames[1]!);

		// Only row 1 should have changed
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('row 1');
		expect(diff[0]).toContain('Body v1');
		expect(diff[0]).toContain('Body v2');

		tui.stop();
	});

	it('serializeFrame produces readable output', async () => {
		const term = new FrameCapturingTerminal(40, 3);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Hello', 'World'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		const serialized = serializeFrame(frame);

		expect(serialized).toContain('Frame 0');
		expect(serialized).toContain('┌');
		expect(serialized).toContain('Hello');
		expect(serialized).toContain('└');

		tui.stop();
	});

	it('differential update has fewer bytes than first render', async () => {
		const term = new FrameCapturingTerminal(40, 5);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2', 'Line 3', 'Line 4'];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Change only one line
		comp.lines = ['Line 1', 'Line 2', 'CHANGED', 'Line 4'];
		tui.requestRender();

		await wait();
		await term.flush();

		const frames = term.getFrames();
		expect(frames[1]!.writeBytes).toBeLessThan(frames[0]!.writeBytes);

		tui.stop();
	});

	it('spinner animation: multiple frame transitions', async () => {
		const spinnerChars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
		const term = new FrameCapturingTerminal(40, 3);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = [`${spinnerChars[0]} Loading...`];
		tui.addChild(comp);
		tui.start();

		await wait();
		await term.flush();

		// Animate through several frames
		for (let i = 1; i < 6; i++) {
			comp.lines = [`${spinnerChars[i % spinnerChars.length]} Loading...`];
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBe(6);

		// Each frame should show a different spinner character
		for (let i = 0; i < 6; i++) {
			const expected = spinnerChars[i % spinnerChars.length]!;
			expect(frames[i]!.viewport[0]).toContain(expected);
		}

		// All updates after first should be differential
		for (let i = 1; i < 6; i++) {
			expect(frames[i]!.isFull).toBe(false);
		}

		tui.stop();
	});
});

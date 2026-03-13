import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { TUI } from '../src/renderer/tui.js';
import { render } from '../src/reconciler/render.js';
import { Box } from '../src/components/Box.js';
import { Text } from '../src/components/Text.js';
import { StreamingPanel } from '../src/components/StreamingPanel.js';
import type { Terminal } from '../src/terminal/terminal.js';

// --- FrameCapturingTerminal ---

interface Frame {
	index: number;
	viewport: string[];
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

	constructor(cols = 80, rows = 20) {
		this._cols = cols;
		this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	}

	get kittyProtocolActive() { return false; }
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
			this.frames.push({
				index: this.frameIndex++,
				viewport: this.getViewport(),
			});
			this.pendingCapture = false;
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

function wait(ms: number) { return new Promise(r => setTimeout(r, ms)); }

function countNonEmptyLines(viewport: string[]): number {
	return viewport.filter(l => l.trim().length > 0).length;
}

function countBlankLinesBetweenContent(viewport: string[]): number {
	// Find first and last non-empty lines, count blanks between them
	const nonEmpty = viewport.map((l, i) => ({ l, i })).filter(x => x.l.trim().length > 0);
	if (nonEmpty.length < 2) return 0;
	const first = nonEmpty[0]!.i;
	const last = nonEmpty[nonEmpty.length - 1]!.i;
	let blanks = 0;
	for (let i = first; i <= last; i++) {
		if (viewport[i]!.trim().length === 0) blanks++;
	}
	return blanks;
}

describe('StreamingPanel rendering (frame capture)', () => {
	it('should not stretch content vertically when content < viewport', async () => {
		const COLS = 60;
		const ROWS = 20;
		const VIEWPORT_HEIGHT = 15; // simulated viewport
		const terminal = new FrameCapturingTerminal(COLS, ROWS);

		// 5 lines of content in a viewport of 15
		const content = Array.from({ length: 5 }, (_, i) => `Line ${i + 1}: content`).join('\n');

		const App = () => (
			<StreamingPanel content={content} streaming={false} height={VIEWPORT_HEIGHT}>
				{(visible) => <Text>{visible}</Text>}
			</StreamingPanel>
		);

		const inst = render(<App />, { terminal });
		await wait(100);
		await terminal.flush();

		const frame = terminal.getLastFrame();
		expect(frame).toBeDefined();

		const viewport = frame!.viewport;
		const nonEmpty = countNonEmptyLines(viewport);
		const blanksBetween = countBlankLinesBetweenContent(viewport);

		// Content is 5 lines — should not be padded to 15 or 20
		expect(nonEmpty).toBeLessThanOrEqual(6); // 5 content + maybe 1 hint
		expect(blanksBetween).toBe(0); // no blank lines between content

		inst.unmount();
	});

	it('should not stretch when scrollbar is visible', async () => {
		const COLS = 60;
		const ROWS = 25;
		const VIEWPORT_HEIGHT = 10;
		const terminal = new FrameCapturingTerminal(COLS, ROWS);

		// 20 lines in a viewport of 10 — scrollbar should show
		const content = Array.from({ length: 20 }, (_, i) => `Line ${i + 1}: some content here`).join('\n');

		const App = () => (
			<StreamingPanel content={content} streaming={false} height={VIEWPORT_HEIGHT}>
				{(visible) => <Text>{visible}</Text>}
			</StreamingPanel>
		);

		const inst = render(<App />, { terminal });
		await wait(100);
		await terminal.flush();

		const frame = terminal.getLastFrame();
		expect(frame).toBeDefined();

		const viewport = frame!.viewport;
		const nonEmpty = countNonEmptyLines(viewport);
		const blanksBetween = countBlankLinesBetweenContent(viewport);

		// Should show ~10 visible lines + scrollbar + hint, NOT 25 rows
		expect(nonEmpty).toBeLessThanOrEqual(12);
		// No blank lines between content rows (the stretching bug)
		expect(blanksBetween).toBe(0);

		inst.unmount();
	});

	it('content should grow without stretching during streaming', async () => {
		const COLS = 60;
		const ROWS = 25;
		const VIEWPORT_HEIGHT = 10;
		const terminal = new FrameCapturingTerminal(COLS, ROWS);

		let updateContent: (c: string) => void;

		const App = () => {
			const [content, setContent] = useState('Line 1');
			updateContent = setContent;
			return (
				<StreamingPanel content={content} streaming={true} height={VIEWPORT_HEIGHT}>
					{(visible) => <Text>{visible}</Text>}
				</StreamingPanel>
			);
		};

		const inst = render(<App />, { terminal });
		await wait(50);
		await terminal.flush();

		// Frame 1: 1 line
		const frame1 = terminal.getLastFrame()!;
		const nonEmpty1 = countNonEmptyLines(frame1.viewport);

		// Add more lines (simulate streaming)
		const lines3 = Array.from({ length: 3 }, (_, i) => `Line ${i + 1}`).join('\n');
		updateContent!(lines3);
		await wait(50);
		await terminal.flush();

		const frame2 = terminal.getLastFrame()!;
		const blanks2 = countBlankLinesBetweenContent(frame2.viewport);
		expect(blanks2).toBe(0);

		// Grow beyond viewport
		const lines15 = Array.from({ length: 15 }, (_, i) => `Line ${i + 1}`).join('\n');
		updateContent!(lines15);
		await wait(50);
		await terminal.flush();

		const frame3 = terminal.getLastFrame()!;
		const blanks3 = countBlankLinesBetweenContent(frame3.viewport);
		const nonEmpty3 = countNonEmptyLines(frame3.viewport);

		// Should show ~10 lines (viewport), no stretching
		expect(blanks3).toBe(0);
		expect(nonEmpty3).toBeLessThanOrEqual(12);

		inst.unmount();
	});
});

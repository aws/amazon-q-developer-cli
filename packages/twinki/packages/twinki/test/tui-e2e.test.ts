import { describe, it, expect } from 'vitest';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import type { Terminal } from '../src/terminal/terminal.js';

/** Minimal virtual terminal for testing */
class TestTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private resizeHandler?: () => void;
	private _cols: number;
	private _rows: number;

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
	stop() { this.inputHandler = undefined; this.resizeHandler = undefined; }
	async drainInput() {}
	write(data: string) { this.xterm.write(data); }
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
		return new Promise(resolve => this.xterm.write('', resolve));
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
}

/** Simple component that renders static lines */
class StaticComponent implements Component {
	constructor(private lines: string[]) {}
	render(_width: number): string[] { return this.lines; }
	invalidate() {}
}

/** Component that can change content */
class MutableComponent implements Component {
	lines: string[] = [];
	render(_width: number): string[] { return this.lines; }
	invalidate() {}
}

describe('TUI + VirtualTerminal E2E', () => {
	it('renders initial content', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new StaticComponent(['Hello, Twinki!', 'Line 2']);
		tui.addChild(comp);
		tui.start();

		// Wait for nextTick render + xterm flush
		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		const viewport = term.getViewport();
		expect(viewport[0]).toContain('Hello, Twinki!');
		expect(viewport[1]).toContain('Line 2');

		tui.stop();
	});

	it('differential update only changes modified lines', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2', 'Line 3'];
		tui.addChild(comp);
		tui.start();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		let viewport = term.getViewport();
		expect(viewport[0]).toContain('Line 1');
		expect(viewport[2]).toContain('Line 3');

		// Change only middle line
		comp.lines = ['Line 1', 'CHANGED', 'Line 3'];
		tui.requestRender();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		viewport = term.getViewport();
		expect(viewport[0]).toContain('Line 1');
		expect(viewport[1]).toContain('CHANGED');
		expect(viewport[2]).toContain('Line 3');

		tui.stop();
	});

	it('handles content growth', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1'];
		tui.addChild(comp);
		tui.start();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		// Grow content
		comp.lines = ['Line 1', 'Line 2', 'Line 3', 'Line 4'];
		tui.requestRender();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		const viewport = term.getViewport();
		expect(viewport[3]).toContain('Line 4');

		tui.stop();
	});

	it('handles content shrink', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['Line 1', 'Line 2', 'Line 3'];
		tui.addChild(comp);
		tui.start();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		// Shrink content
		comp.lines = ['Line 1'];
		tui.requestRender();

		await new Promise(r => setTimeout(r, 10));
		await term.flush();

		const viewport = term.getViewport();
		expect(viewport[0]).toContain('Line 1');
		// Old lines should be cleared
		expect(viewport[1]?.trim()).toBe('');

		tui.stop();
	});

	it('synchronized output wraps every frame', async () => {
		const writes: string[] = [];
		const term = new TestTerminal(40, 10);
		const origWrite = term.write.bind(term);
		term.write = (data: string) => { writes.push(data); origWrite(data); };

		const tui = new TUI(term);
		const comp = new StaticComponent(['test']);
		tui.addChild(comp);
		tui.start();

		await new Promise(r => setTimeout(r, 10));

		// Find the render write (not the hideCursor write)
		const renderWrite = writes.find(w => w.includes('test'));
		expect(renderWrite).toBeDefined();
		expect(renderWrite).toContain('\x1b[?2026h'); // sync start
		expect(renderWrite).toContain('\x1b[?2026l'); // sync end

		tui.stop();
	});

	it('input dispatch reaches focused component', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const received: string[] = [];
		const comp: Component = {
			render: () => ['waiting for input...'],
			invalidate: () => {},
			handleInput: (data: string) => { received.push(data); },
		};
		tui.addChild(comp);
		tui.setFocus(comp);
		tui.start();

		await new Promise(r => setTimeout(r, 10));

		term.sendInput('a');
		expect(received).toEqual(['a']);

		tui.stop();
	});
});

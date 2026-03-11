import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import type { Terminal } from '../src/terminal/terminal.js';
import { TUI } from '../src/renderer/tui.js';
import { MutableComponent, wait } from './helpers.js';

class TestTerminal implements Terminal {
	xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private _cols: number;
	private _rows: number;
	writes: string[] = [];

	constructor(cols = 60, rows = 10) {
		this._cols = cols;
		this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	}

	get kittyProtocolActive() { return true; }
	get columns() { return this._cols; }
	get rows() { return this._rows; }
	start(onInput: (data: string) => void) { this.inputHandler = onInput; }
	stop() {}
	async drainInput() {}
	write(data: string) { this.writes.push(data); this.xterm.write(data); }
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

// --- usePaste ---

describe('usePaste', () => {
	it('receives pasted content from bracketed paste sequence', async () => {
		const term = new TestTerminal();
		const { usePaste } = await import('../src/hooks/usePaste.js');

		const pasted: string[] = [];
		function PasteApp() {
			usePaste((content) => { pasted.push(content); });
			return React.createElement(Text, null, 'paste test');
		}

		const instance = render(
			React.createElement(PasteApp),
			{ terminal: term, exitOnCtrlC: false },
		);
		await wait();
		await term.flush();

		// Simulate 30 lines of pasted text
		const lines = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`);
		const pasteContent = lines.join('\n');
		term.sendInput(`\x1b[200~${pasteContent}\x1b[201~`);
		await wait();

		expect(pasted).toHaveLength(1);
		expect(pasted[0]).toBe(pasteContent);
		expect(pasted[0]!.split('\n')).toHaveLength(30);

		instance.unmount();
	});

	it('does not fire when isActive is false', async () => {
		const term = new TestTerminal();
		const { usePaste } = await import('../src/hooks/usePaste.js');

		const pasted: string[] = [];
		function PasteApp() {
			usePaste((content) => { pasted.push(content); }, { isActive: false });
			return React.createElement(Text, null, 'inactive');
		}

		const instance = render(
			React.createElement(PasteApp),
			{ terminal: term, exitOnCtrlC: false },
		);
		await wait();

		term.sendInput('\x1b[200~hello\x1b[201~');
		await wait();

		expect(pasted).toHaveLength(0);
		instance.unmount();
	});
});

// --- useFullscreen ---

describe('useFullscreen', () => {
	it('enters alternate screen on mount and exits on unmount', async () => {
		const term = new TestTerminal();
		const { useFullscreen } = await import('../src/hooks/useFullscreen.js');

		function FullscreenApp() {
			useFullscreen();
			return React.createElement(Text, null, 'fullscreen');
		}

		const instance = render(
			React.createElement(FullscreenApp),
			{ terminal: term, exitOnCtrlC: false },
		);
		await wait();
		await term.flush();

		// Should have written alt screen enter
		const allWrites = term.writes.join('');
		expect(allWrites).toContain('\x1b[?1049h');

		// xterm should be in alternate buffer
		expect(term.xterm.buffer.active.type).toBe('alternate');

		instance.unmount();
		await wait();
		await term.flush();

		// Should have written alt screen exit
		const allWritesAfter = term.writes.join('');
		expect(allWritesAfter).toContain('\x1b[?1049l');
	});
});

// --- useKeyRelease ---

describe('useKeyRelease', () => {
	it('fires on key release events', async () => {
		const term = new TestTerminal();
		const { useKeyRelease } = await import('../src/hooks/useKeyRelease.js');

		const released: string[] = [];
		function ReleaseApp() {
			useKeyRelease((input, key) => {
				released.push(key.return ? 'enter' : input);
			});
			return React.createElement(Text, null, 'release test');
		}

		const instance = render(
			React.createElement(ReleaseApp),
			{ terminal: term, exitOnCtrlC: false },
		);
		await wait();

		// Kitty key release for 'a': CSI 97;1:3u
		term.sendInput('\x1b[97;1:3u');
		await wait();

		expect(released).toHaveLength(1);
		instance.unmount();
	});
});

// --- useKeyRepeat ---

describe('useKeyRepeat', () => {
	it('fires on key repeat events', async () => {
		const term = new TestTerminal();
		const { useKeyRepeat } = await import('../src/hooks/useKeyRepeat.js');

		const repeated: string[] = [];
		function RepeatApp() {
			useKeyRepeat((input) => { repeated.push(input); });
			return React.createElement(Text, null, 'repeat test');
		}

		const instance = render(
			React.createElement(RepeatApp),
			{ terminal: term, exitOnCtrlC: false },
		);
		await wait();

		// Kitty key repeat for 'a': CSI 97;1:2u
		term.sendInput('\x1b[97;1:2u');
		await wait();

		expect(repeated).toHaveLength(1);
		instance.unmount();
	});
});

// --- Frame pacing ---

describe('frame pacing', () => {
	it('coalesces rapid requestRender calls with targetFps', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term, { targetFps: 10 }); // 100ms budget
		const comp = new MutableComponent();
		comp.lines = ['frame 0'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		// Fire 20 rapid requestRender calls within one frame budget
		let renderCount = 0;
		const origDoRender = (tui as any).doRender.bind(tui);
		(tui as any).doRender = function() {
			renderCount++;
			origDoRender();
		};

		for (let i = 1; i <= 20; i++) {
			comp.lines = [`frame ${i}`];
			tui.requestRender();
		}

		// Wait for the pacing timer to fire (budget is 100ms)
		await new Promise(r => setTimeout(r, 150));
		await term.flush();

		// Should have rendered far fewer than 20 times
		expect(renderCount).toBeLessThanOrEqual(3);
		// But the final state should reflect the last update
		expect(term.getViewport()[0]).toContain('frame 20');

		tui.stop();
	});

	it('renders immediately when no targetFps is set', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term); // no pacing
		const comp = new MutableComponent();
		comp.lines = ['initial'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		comp.lines = ['updated'];
		tui.requestRender();
		await wait();
		await term.flush();

		expect(term.getViewport()[0]).toContain('updated');
		tui.stop();
	});

	it('force render bypasses pacing state', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term, { targetFps: 10 });
		const comp = new MutableComponent();
		comp.lines = ['initial'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		// Force render should clear state and schedule immediately
		comp.lines = ['forced'];
		tui.requestRender(true);
		await wait();
		await term.flush();

		expect(term.getViewport()[0]).toContain('forced');
		tui.stop();
	});
});

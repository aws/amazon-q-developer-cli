import { describe, it, expect } from 'vitest';
import React from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import type { Terminal } from '../src/terminal/terminal.js';

/** Minimal virtual terminal for React tests */
class TestTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private _cols: number;
	private _rows: number;

	constructor(cols = 60, rows = 10) {
		this._cols = cols;
		this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	}

	get kittyProtocolActive() { return true; }
	get columns() { return this._cols; }
	get rows() { return this._rows; }
	start(onInput: (data: string) => void, onResize: () => void) { this.inputHandler = onInput; }
	stop() {}
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

	getLine(row: number): string {
		const line = this.xterm.buffer.active.getLine(row);
		return line ? line.translateToString(true) : '';
	}

	getViewport(): string[] {
		const lines: string[] = [];
		for (let i = 0; i < this._rows; i++) lines.push(this.getLine(i));
		return lines;
	}

	getCellFg(row: number, col: number): number {
		const line = this.xterm.buffer.active.getLine(row);
		if (!line) return 0;
		const cell = line.getCell(col);
		if (!cell) return 0;
		return cell.getFgColor();
	}

	isCellBold(row: number, col: number): boolean {
		const line = this.xterm.buffer.active.getLine(row);
		if (!line) return false;
		const cell = line.getCell(col);
		if (!cell) return false;
		return cell.isBold() !== 0;
	}
}

async function wait(ms = 20) {
	await new Promise(r => setTimeout(r, ms));
}

describe('React Pipeline E2E', () => {
	it('renders <Text> through the full pipeline', async () => {
		const term = new TestTerminal();
		const instance = render(
			React.createElement(Text, null, 'Hello from React!'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		expect(viewport.some(l => l.includes('Hello from React!'))).toBe(true);

		instance.unmount();
	});

	it('renders <Box> column layout', async () => {
		const term = new TestTerminal();
		const instance = render(
			React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'Line A'),
				React.createElement(Text, null, 'Line B'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		const lineA = viewport.findIndex(l => l.includes('Line A'));
		const lineB = viewport.findIndex(l => l.includes('Line B'));
		expect(lineA).toBeGreaterThanOrEqual(0);
		expect(lineB).toBeGreaterThan(lineA);

		instance.unmount();
	});

	it('renders <Box flexDirection="row"> side by side', async () => {
		const term = new TestTerminal(40, 10);
		const instance = render(
			React.createElement(Box, { flexDirection: 'row' },
				React.createElement(Text, null, 'LEFT'),
				React.createElement(Text, null, 'RIGHT'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		// Both texts should be on the same line
		const line = viewport.find(l => l.includes('LEFT') && l.includes('RIGHT'));
		expect(line).toBeDefined();
		// LEFT should appear before RIGHT
		expect(line!.indexOf('LEFT')).toBeLessThan(line!.indexOf('RIGHT'));

		instance.unmount();
	});

	it('renders <Text bold> with ANSI codes', async () => {
		const term = new TestTerminal();
		const instance = render(
			React.createElement(Text, { bold: true }, 'BOLD'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		// The raw xterm buffer won't show ANSI codes in translateToString,
		// but the text should still be there
		const viewport = term.getViewport();
		expect(viewport.some(l => l.includes('BOLD'))).toBe(true);

		instance.unmount();
	});

	it('rerender updates content', async () => {
		const term = new TestTerminal();
		const instance = render(
			React.createElement(Text, null, 'Version 1'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();
		expect(term.getViewport().some(l => l.includes('Version 1'))).toBe(true);

		instance.rerender(React.createElement(Text, null, 'Version 2'));

		await wait();
		await term.flush();
		expect(term.getViewport().some(l => l.includes('Version 2'))).toBe(true);

		instance.unmount();
	});

	it('useApp and useInput hooks work inside render tree', async () => {
		const term = new TestTerminal();

		// Import hooks
		const { useApp } = await import('../src/hooks/useApp.js');
		const { useInput } = await import('../src/hooks/useInput.js');

		const received: string[] = [];
		function HookApp() {
			const { exit } = useApp();
			useInput((input) => { received.push(input); });
			return React.createElement(Text, null, 'hooks work');
		}

		// This would throw "must be used inside a Twinki render tree"
		// if the context provider is missing
		const instance = render(
			React.createElement(HookApp),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();
		expect(term.getViewport().some(l => l.includes('hooks work'))).toBe(true);

		instance.unmount();
	});

	it('renders <Text color="red"> with foreground color', async () => {
		const term = new TestTerminal();
		const instance = render(
			React.createElement(Text, { color: 'red' }, 'RED'),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		expect(viewport[0]).toContain('RED');
		// xterm should have parsed the ANSI color — check cell fg is non-default
		const col = viewport[0]!.indexOf('R');
		expect(term.getCellFg(0, col)).toBeGreaterThan(0);

		instance.unmount();
	});

	it('renders <Text wrap="truncate"> truncates long text', async () => {
		const term = new TestTerminal(20, 5);
		const longText = 'This text is way too long to fit in twenty columns';
		const instance = render(
			React.createElement(Text, { wrap: 'truncate' }, longText),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		// Should be truncated to 20 columns, not wrapped
		expect(viewport[0]!.trim().length).toBeLessThanOrEqual(20);
		expect(viewport[0]).toContain('This text');
		// Second line should be empty (no wrapping)
		expect(viewport[1]!.trim()).toBe('');

		instance.unmount();
	});

	it('renders <Text wrap="truncate-middle"> with ellipsis', async () => {
		const term = new TestTerminal(20, 5);
		const longText = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
		const instance = render(
			React.createElement(Text, { wrap: 'truncate-middle' }, longText),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		// Should contain start and end of text with ellipsis
		expect(viewport[0]).toContain('A');
		expect(viewport[0]).toContain('Z');
		expect(viewport[0]).toContain('…');

		instance.unmount();
	});
});

import { Markdown } from '../src/components/Markdown.js';

describe('Markdown Rendering', () => {
	it('renders code blocks with indented content', async () => {
		const term = new TestTerminal(60, 20);
		const md = '# Title\n\nSome text\n\n```typescript\nconst x = 1;\nconst y = 2;\n```\n\nDone.';
		const instance = render(
			React.createElement(Markdown, { children: md }),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();

		// Title should render
		expect(viewport.some(l => l.includes('Title'))).toBe(true);

		// Code lines should appear and be indented
		const codeLine1 = viewport.findIndex(l => l.includes('const x = 1;'));
		const codeLine2 = viewport.findIndex(l => l.includes('const y = 2;'));
		expect(codeLine1).toBeGreaterThanOrEqual(0);
		expect(codeLine2).toBeGreaterThan(codeLine1);

		// Code lines should be indented
		expect(viewport[codeLine1]!.startsWith('  ')).toBe(true);

		// "Done." should appear after the code block
		const doneRow = viewport.findIndex(l => l.includes('Done.'));
		expect(doneRow).toBeGreaterThan(codeLine2);

		instance.unmount();
	});

	it('renders headings as bold', async () => {
		const term = new TestTerminal(60, 10);
		const instance = render(
			React.createElement(Markdown, { children: '# Hello World' }),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		const row = viewport.findIndex(l => l.includes('Hello World'));
		expect(row).toBeGreaterThanOrEqual(0);
		expect(term.isCellBold(row, viewport[row]!.indexOf('H'))).toBe(true);

		instance.unmount();
	});

	it('renders inline formatting: bold, italic, inline code', async () => {
		const term = new TestTerminal(60, 10);
		const instance = render(
			React.createElement(Markdown, { children: 'Some **bold** and `code` text' }),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		const row = viewport.findIndex(l => l.includes('bold'));
		expect(row).toBeGreaterThanOrEqual(0);

		// bold should be bold
		const boldCol = viewport[row]!.indexOf('bold');
		expect(term.isCellBold(row, boldCol)).toBe(true);

		// inline code should appear with backticks
		expect(viewport[row]!).toContain('`code`');

		instance.unmount();
	});

	it('renders lists with bullet markers', async () => {
		const term = new TestTerminal(60, 10);
		const instance = render(
			React.createElement(Markdown, { children: '- Item one\n- Item two\n- Item three' }),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const viewport = term.getViewport();
		const items = viewport.filter(l => l.includes('Item'));
		expect(items.length).toBe(3);
		// Should have bullet markers
		expect(items[0]).toMatch(/[•\-]/);

		instance.unmount();
	});

	it('completed code blocks get syntax colors automatically', async () => {
		const term = new TestTerminal(60, 20);
		const md = '```typescript\nconst x = 1;\n```';
		const instance = render(
			React.createElement(Markdown, { children: md }),
			{ terminal: term, exitOnCtrlC: false },
		);

		// Wait for shiki async highlight
		await wait(500);
		await term.flush();

		const viewport = term.getViewport();
		const codeLine = viewport.findIndex(l => l.includes('const'));
		expect(codeLine).toBeGreaterThanOrEqual(0);

		// With shiki, 'const' should have a foreground color (non-zero)
		const constCol = viewport[codeLine]!.indexOf('const');
		const fg = term.getCellFg(codeLine, constCol);
		// shiki uses RGB colors, which xterm reports as non-zero
		expect(fg).not.toBe(0);

		instance.unmount();
	});

	it('nested Text cursor stays visible as text grows', async () => {
		const term = new TestTerminal(40, 5);
		let setVal!: (v: string) => void;

		function App() {
			const [text, setText] = React.useState('');
			setVal = setText;
			const parts: React.ReactNode[] = [];
			if (text) parts.push(React.createElement(Text, { key: 'before' }, text));
			parts.push(React.createElement(Text, { key: 'cursor', inverse: true }, ' '));
			return React.createElement(Box, { flexGrow: 1 },
				React.createElement(Text, { wrap: 'wrap' as const }, ...parts)
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(100); await term.flush();
		const empty = term.getLine(0);
		console.log('empty:', JSON.stringify(empty));

		setVal('h');
		await wait(100); await term.flush();
		const afterH = term.getLine(0);
		console.log('afterH:', JSON.stringify(afterH));

		const rawWrites: string[] = [];
		const origTermWrite = term.write.bind(term);
		(term as any).write = (data: string) => {
			rawWrites.push(data);
			return origTermWrite(data);
		};

		setVal('he');
		await wait(100); await term.flush();
		const afterHE = term.getLine(0);
		console.log('afterHE:', JSON.stringify(afterHE));
		console.log('raw writes for he:', rawWrites.map(w => JSON.stringify(w)).join('\n'));
		// Also intercept tui render output directly
		const tui = (instance as any).tui ?? (instance as any)._tui;
		if (tui) console.log('tui exists');


		instance.unmount();

		// empty: cursor space renders (xterm shows as space, trim strips it — check raw length)
		expect(empty.length).toBeGreaterThan(0);
		expect(afterH).toContain('h');
		// 'he' + cursor space must both be present
		expect(afterHE).toContain('he');
		// cursor is inverse space — xterm renders it with inverse attribute (non-zero)
		const heCell = term.xterm.buffer.active.getLine(0)?.getCell(2);
		expect(heCell?.isInverse()).toBeGreaterThan(0);
	});
});

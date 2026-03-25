import { describe, it, expect } from 'vitest';
import React from 'react';
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import type { Terminal } from '../src/terminal/terminal.js';

class TestTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
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
	start(onInput: (data: string) => void) { this.inputHandler = onInput; }
	stop() {}
	async drainInput() {}
	write(data: string) { this.xterm.write(data); }
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

async function wait(ms = 30) {
	await new Promise(r => setTimeout(r, ms));
}

describe('last character eaten at wrap boundary', () => {
	it('simple case: line filling exactly to width preserves last char', async () => {
		// Use a width where the text fills exactly
		const termWidth = 22; // "# " (2) + 20 content chars
		const text = 'abcdefghijklmnopqrst'; // exactly 20 chars
		const term = new TestTerminal(termWidth, 5);

		const inst = render(
			React.createElement(Box, { flexDirection: 'row', width: termWidth },
				React.createElement(Box, { width: 1 },
					React.createElement(Text, {}, '#')
				),
				React.createElement(Box, { flexGrow: 1, marginLeft: 1 },
					React.createElement(Text, { wrap: 'wrap' }, text)
				)
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const vp = term.getViewport();
		const line = vp[0]!.replace(/\s+$/, '');
		console.log(`Line: "${line}" (length=${line.length})`);
		
		// Last char should be 't'
		expect(line[line.length - 1]).toBe('t');
		expect(line.length).toBe(termWidth);

		inst.unmount();
	});

	it('wrapped line: last char before wrap is preserved', async () => {
		const termWidth = 22; // content width = 20
		const text = 'abcdefghijklmnopqrstuvwxyz'; // 26 chars, wraps at 20
		const term = new TestTerminal(termWidth, 5);

		const inst = render(
			React.createElement(Box, { flexDirection: 'row', width: termWidth },
				React.createElement(Box, { width: 1 },
					React.createElement(Text, {}, '#')
				),
				React.createElement(Box, { flexGrow: 1, marginLeft: 1 },
					React.createElement(Text, { wrap: 'wrap' }, text)
				)
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const vp = term.getViewport();
		const line1 = vp[0]!.replace(/\s+$/, '');
		const line2 = vp[1]!.replace(/\s+$/, '');
		console.log(`Line1: "${line1}" (length=${line1.length})`);
		console.log(`Line2: "${line2}" (length=${line2.length})`);
		
		// Line 1 should end with 't' (20th char)
		expect(line1[line1.length - 1]).toBe('t');
		expect(line1.length).toBe(termWidth);
		
		// Line 2 should start with "  uvwxyz" (margin + remaining)
		expect(line2.trim()).toBe('uvwxyz');

		// ALL characters must be present
		const allContent = line1.slice(2) + line2.slice(2).trim();
		expect(allContent).toBe('abcdefghijklmnopqrstuvwxyz');

		inst.unmount();
	});

	it('word-wrapped: last char of line before wrap is preserved', async () => {
		const termWidth = 22; // content width = 20
		// "hello world testing" = 19 chars, fits
		// "hello world testing x" = 21 chars, wraps
		const text = 'hello world testing x';
		const term = new TestTerminal(termWidth, 5);

		const inst = render(
			React.createElement(Box, { flexDirection: 'row', width: termWidth },
				React.createElement(Box, { width: 1 },
					React.createElement(Text, {}, '#')
				),
				React.createElement(Box, { flexGrow: 1, marginLeft: 1 },
					React.createElement(Text, { wrap: 'wrap' }, text)
				)
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait();
		await term.flush();

		const vp = term.getViewport();
		const line1 = vp[0]!.replace(/\s+$/, '');
		const line2 = vp[1]!.replace(/\s+$/, '');
		console.log(`Line1: "${line1}" (length=${line1.length})`);
		console.log(`Line2: "${line2}" (length=${line2.length})`);
		
		// "hello world testing" = 19 chars, fits in 20
		// Line 1 should contain "hello world testing"
		expect(line1).toContain('testing');
		// Line 2 should contain "x"
		expect(line2.trim()).toBe('x');

		inst.unmount();
	});

	// This is the key test - reproduce the exact user scenario
	it('REPRO: characters at wrap boundary are preserved', async () => {
		for (const termWidth of [40, 50, 60, 70, 80, 90, 100, 110, 120]) {
			const contentWidth = termWidth - 2;
			const text = 'S3 — Object storage with virtually unlimited capacity. Supports versioning, lifecycle policies, and multiple storage classes (Standard, IA,Glacier, etc.).';
			const term = new TestTerminal(termWidth, 20);

			const inst = render(
				React.createElement(Box, { flexDirection: 'row', width: termWidth },
					React.createElement(Box, { width: 1 },
						React.createElement(Text, {}, '#')
					),
					React.createElement(Box, { flexGrow: 1, marginLeft: 1 },
						React.createElement(Text, { wrap: 'wrap' }, text)
					)
				),
				{ terminal: term, exitOnCtrlC: false },
			);

			await wait();
			await term.flush();

			const vp = term.getViewport();
			
			// Collect all content (skip bar + margin)
			let allContent = '';
			for (const line of vp) {
				const trimmed = line.replace(/\s+$/, '');
				if (trimmed.length >= 2 && trimmed[0] === '#') {
					allContent += trimmed.slice(2); // skip "# "
				} else if (trimmed.length >= 2 && trimmed.startsWith('  ')) {
					allContent += trimmed.slice(2); // skip "  " margin on continuation lines
				}
			}

			// Check that ALL non-space characters are present
			const originalChars = text.replace(/ /g, '');
			const renderedChars = allContent.replace(/ /g, '');
			
			if (originalChars !== renderedChars) {
				console.log(`\nFAIL at width ${termWidth}:`);
				for (const line of vp) {
					if (line.trim()) console.log(`  "${line.replace(/\s+$/, '')}"`);
				}
				// Find first difference
				for (let i = 0; i < Math.max(originalChars.length, renderedChars.length); i++) {
					if (originalChars[i] !== renderedChars[i]) {
						console.log(`  First diff at ${i}: expected '${originalChars[i]}' got '${renderedChars[i] || 'EOF'}'`);
						console.log(`  Context: ...${originalChars.slice(Math.max(0,i-10), i+10)}...`);
						break;
					}
				}
			}
			
			expect(renderedChars).toBe(originalChars);

			inst.unmount();
		}
	});
});

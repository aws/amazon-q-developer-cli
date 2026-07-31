import React from 'react';
import { describe, expect, it } from 'vitest';
import {
	Box,
	Text,
	render,
	useSelectionCopy,
} from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

class SelectionTerminal extends TestTerminal {
	private rawWrites: string[] = [];

	override write(data: string): void {
		this.rawWrites.push(data);
		super.write(data);
	}

	clearRawWrites(): void {
		this.rawWrites.length = 0;
	}

	writes(): string {
		return this.rawWrites.join('');
	}

	clipboardTexts(): string[] {
		const texts: string[] = [];
		const pattern = /\x1b]52;c;([A-Za-z0-9+/=]*)\x07/g;
		for (const write of this.rawWrites) {
			for (const match of write.matchAll(pattern)) {
				texts.push(Buffer.from(match[1]!, 'base64').toString('utf8'));
			}
		}
		return texts;
	}
}

function mouseDown(x: number, y: number): string {
	return `\x1b[<0;${x + 1};${y + 1}M`;
}

function mouseMove(x: number, y: number): string {
	return `\x1b[<32;${x + 1};${y + 1}M`;
}

function mouseUp(x: number, y: number): string {
	return `\x1b[<0;${x + 1};${y + 1}m`;
}

async function flush(terminal: SelectionTerminal): Promise<void> {
	await wait();
	await terminal.flush();
}

async function drag(
	terminal: SelectionTerminal,
	from: { x: number; y: number },
	to: { x: number; y: number },
): Promise<void> {
	terminal.sendInput(mouseDown(from.x, from.y));
	terminal.sendInput(mouseMove(to.x, to.y));
	terminal.sendInput(mouseUp(to.x, to.y));
	await flush(terminal);
}

describe('renderer text selection', () => {
	it('highlights and copies ANSI-styled text without terminal padding', async () => {
		const terminal = new SelectionTerminal(30, 5);
		const instance = render(
			React.createElement(
				Text,
				{ color: 'red', wrap: 'overflow' },
				'  alpha   ',
			),
			{ terminal, exitOnCtrlC: false, textSelection: true },
		);
		await flush(terminal);
		terminal.clearRawWrites();

		terminal.sendInput(mouseDown(0, 0));
		terminal.sendInput(mouseMove(9, 0));
		await flush(terminal);
		expect(terminal.writes()).toContain('\x1b[7m');
		expect(terminal.clipboardTexts()).toEqual([]);

		terminal.sendInput(mouseUp(9, 0));
		await flush(terminal);
		expect(terminal.clipboardTexts()).toEqual(['  alpha']);

		instance.unmount();
	});

	it('selects complete CJK, combining, and emoji graphemes', async () => {
		const terminal = new SelectionTerminal(30, 5);
		const instance = render(
			React.createElement(Text, { wrap: 'overflow' }, 'A界e\u0301👩‍💻Z'),
			{ terminal, exitOnCtrlC: false, textSelection: true },
		);
		await flush(terminal);
		terminal.clearRawWrites();

		await drag(terminal, { x: 2, y: 0 }, { x: 4, y: 0 });
		expect(terminal.clipboardTexts()).toEqual(['界e\u0301👩‍💻']);

		instance.unmount();
	});

	it('selects across soft-wrapped physical rows as one logical line', async () => {
		const terminal = new SelectionTerminal(10, 5);
		const instance = render(
			React.createElement(Text, { wrap: 'overflow' }, 'abcdefghijklmnop'),
			{
				terminal,
				exitOnCtrlC: false,
				textSelection: true,
				wideLines: true,
			},
		);
		await flush(terminal);
		terminal.clearRawWrites();

		await drag(terminal, { x: 8, y: 0 }, { x: 3, y: 1 });
		expect(terminal.clipboardTexts()).toEqual(['ijklmn']);

		instance.unmount();
	});

	it('clamps selection to the scope where the drag started', async () => {
		const terminal = new SelectionTerminal(30, 5);
		const instance = render(
			React.createElement(
				Box,
				{ flexDirection: 'row', width: 20, height: 2 },
				React.createElement(
					Box,
					{ width: 10, height: 2, selectionScope: true },
					React.createElement(
						Text,
						{ wrap: 'overflow' },
						'abcdefghij\nklmnopqrst',
					),
				),
				React.createElement(
					Box,
					{ width: 10, height: 2, selectionScope: true },
					React.createElement(
						Text,
						{ wrap: 'overflow' },
						'UVWXYZABCD\nEFGHIJKLMN',
					),
				),
			),
			{ terminal, exitOnCtrlC: false, textSelection: true },
		);
		await flush(terminal);
		terminal.clearRawWrites();

		await drag(terminal, { x: 2, y: 0 }, { x: 15, y: 1 });
		expect(terminal.clipboardTexts()).toEqual(['cdefghij\nklmnopqrst']);

		instance.unmount();
	});

	it('suppresses clicks after a drag but preserves stationary clicks', async () => {
		const terminal = new SelectionTerminal(30, 5);
		let clicks = 0;
		const instance = render(
			React.createElement(
				Box,
				{ onClick: () => clicks++ },
				React.createElement(Text, null, 'click target'),
			),
			{ terminal, exitOnCtrlC: false, textSelection: true },
		);
		await flush(terminal);
		terminal.clearRawWrites();

		await drag(terminal, { x: 0, y: 0 }, { x: 4, y: 0 });
		expect(clicks).toBe(0);

		terminal.sendInput(mouseDown(1, 0));
		terminal.sendInput(mouseUp(1, 0));
		await flush(terminal);
		expect(clicks).toBe(1);

		instance.unmount();
	});

	it('notifies useSelectionCopy after copying', async () => {
		const terminal = new SelectionTerminal(30, 5);
		const copied: string[] = [];

		function App(): React.ReactElement {
			useSelectionCopy((text) => copied.push(text));
			return React.createElement(Text, null, 'hook callback');
		}

		const instance = render(React.createElement(App), {
			terminal,
			exitOnCtrlC: false,
			textSelection: true,
		});
		await flush(terminal);

		await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
		expect(copied).toEqual(['hook']);

		instance.unmount();
	});
});

import { describe, it, expect, afterEach } from 'vitest';
import { TestTerminal } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';
import { Input } from '../src/components/Input.js';
import { Editor } from '../src/components/Editor.js';

const wait = (ms = 20) => new Promise((r) => setTimeout(r, ms));

/**
 * A software cursor drawn as a reverse-video cell cancels out against the
 * terminal's own cursor parked on the same cell. The renderer strips the
 * inversion from the marker cell whenever the hardware cursor is visible, so
 * components can paint their cursor unconditionally.
 */
describe('hardware cursor inverse suppression E2E', () => {
	let term: TestTerminal;
	let tui: TUI;

	afterEach(() => {
		tui?.stop();
	});

	async function renderInput(showHardwareCursor: boolean) {
		term = new TestTerminal(40, 5);
		tui = new TUI(term, showHardwareCursor);
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();
		term.sendInput('hi');
		tui.requestRender();
		await wait();
		await term.flush();
	}

	async function renderEditor(showHardwareCursor: boolean) {
		term = new TestTerminal(40, 8);
		tui = new TUI(term, showHardwareCursor);
		const editor = new Editor({ terminalRows: 8 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();
		term.sendInput('hi');
		tui.requestRender();
		await wait();
		await term.flush();
	}

	it('Input cursor cell is not software-inverted when the hardware cursor is visible', async () => {
		await renderInput(true);
		expect(term.getCursorCell().inverse).toBe(false);
	});

	it('Input cursor cell stays software-inverted when the hardware cursor is hidden', async () => {
		await renderInput(false);
		expect(term.getCursorCell().inverse).toBe(true);
	});

	it('Editor cursor cell is not software-inverted when the hardware cursor is visible', async () => {
		await renderEditor(true);
		expect(term.getCursorCell().inverse).toBe(false);
	});

	it('Editor cursor cell stays software-inverted when the hardware cursor is hidden', async () => {
		await renderEditor(false);
		expect(term.getCursorCell().inverse).toBe(true);
	});
});

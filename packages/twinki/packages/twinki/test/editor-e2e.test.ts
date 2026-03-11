import { describe, it, expect, afterEach } from 'vitest';
import { TestTerminal } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';
import { Editor } from '../src/components/Editor.js';

const wait = (ms = 10) => new Promise(r => setTimeout(r, ms));

describe('Editor E2E', () => {
	let term: TestTerminal;
	let tui: TUI;

	afterEach(() => { tui?.stop(); });

	function setup(cols = 40, rows = 15) {
		term = new TestTerminal(cols, rows);
		tui = new TUI(term);
	}

	it('should render empty editor with borders', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();
		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport[0]).toContain('─');
		expect(frame.viewport[2]).toContain('─');
	});

	it('should show typed text', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		term.sendInput('hello');
		tui.requestRender();
		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport[1]).toContain('hello');
	});

	it('should handle multi-line editing', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		term.sendInput('line1');
		term.sendInput('\x1b[13;2u'); // Shift+Enter
		term.sendInput('line2');
		tui.requestRender();
		await wait();
		await term.flush();

		expect(editor.getLines()).toEqual(['line1', 'line2']);
	});

	it('should handle submit', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		let submitted = '';
		editor.onSubmit = (v) => { submitted = v; };
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		term.sendInput('test');
		term.sendInput('\r');

		expect(submitted).toBe('test');
	});

	it('should handle undo', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		term.sendInput('hello');
		term.sendInput(' ');
		term.sendInput('world');
		term.sendInput('\x1b[45;5u'); // Ctrl+- (undo)

		expect(editor.getText()).toBe('hello');
	});

	it('should handle paste markers', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		const longText = Array.from({ length: 15 }, (_, i) => `line ${i}`).join('\n');
		term.sendInput(`\x1b[200~${longText}\x1b[201~`);

		expect(editor.getText()).toContain('[paste #1');
		expect(editor.getExpandedText()).toBe(longText);
	});

	it('should use differential rendering', async () => {
		setup();
		const editor = new Editor({ terminalRows: 15 });
		editor.focused = true;
		tui.addChild(editor);
		tui.setFocus(editor);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('a');
		tui.requestRender();
		await wait();
		await term.flush();

		term.sendInput('b');
		tui.requestRender();
		await wait();
		await term.flush();

		const lastFrame = term.getFrames()[term.getFrames().length - 1]!;
		expect(lastFrame.isFull).toBe(false);
	});
});

import { describe, it, expect, afterEach } from 'vitest';
import { TestTerminal, testDir, dumpAllFrames } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';
import { Input } from '../src/components/Input.js';

const wait = (ms = 10) => new Promise(r => setTimeout(r, ms));

describe('TextInput E2E', () => {
	let term: TestTerminal;
	let tui: TUI;

	afterEach(() => {
		tui?.stop();
	});

	function setup(cols = 40, rows = 5) {
		term = new TestTerminal(cols, rows);
		tui = new TUI(term);
	}

	it('should render empty input with cursor', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport[0]).toContain('> ');
	});

	it('should show typed characters', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('hello');
		tui.requestRender();
		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport[0]).toContain('hello');
	});

	it('should handle backspace', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		term.sendInput('abc');
		tui.requestRender();
		await wait();
		await term.flush();

		term.sendInput('\x7f'); // backspace
		tui.requestRender();
		await wait();
		await term.flush();

		expect(input.getValue()).toBe('ab');
		expect(term.getLastFrame()!.viewport[0]).toContain('ab');
	});

	it('should handle submit', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		let submitted = '';
		input.onSubmit = (v) => { submitted = v; };
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		term.sendInput('test');
		term.sendInput('\r'); // enter
		await wait();
		await term.flush();

		expect(submitted).toBe('test');
	});

	it('should handle undo/redo', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		term.sendInput('hello');
		term.sendInput(' ');
		term.sendInput('world');
		expect(input.getValue()).toBe('hello world');

		// Undo (Ctrl+-)
		term.sendInput('\x1b[45;5u');
		expect(input.getValue()).toBe('hello');
	});

	it('should handle kill and yank', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
		tui.start();

		term.sendInput('hello world');
		term.sendInput('\x01'); // Ctrl+A (home)
		term.sendInput('\x0b'); // Ctrl+K (kill to end)
		expect(input.getValue()).toBe('');

		term.sendInput('\x19'); // Ctrl+Y (yank)
		expect(input.getValue()).toBe('hello world');
	});

	it('should use differential rendering', async () => {
		setup();
		const input = new Input();
		input.focused = true;
		tui.addChild(input);
		tui.setFocus(input);
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

		const frames = term.getFrames();
		const lastFrame = frames[frames.length - 1]!;
		// After initial render, subsequent frames should be differential
		expect(lastFrame.isFull).toBe(false);
	});

	it('should show placeholder when unfocused', async () => {
		setup();
		const input = new Input();
		input.focused = false;
		input.placeholder = 'Type here...';
		tui.addChild(input);
		tui.start();

		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport[0]).toContain('Type here...');
	});
});

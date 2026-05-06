/**
 * Tests for the EditorInput component: controlled/uncontrolled value,
 * submit handling, isActive gating, and line number rendering.
 */
import { describe, it, expect } from 'vitest';
import React from 'react';
import { render } from '../src/reconciler/render.js';
import { TUI } from '../src/renderer/tui.js';
import { TestTerminal, wait } from './helpers.js';

describe('EditorInput', () => {
	it('renders and accepts input', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		let lastValue = '';
		const inst = render(
			React.createElement(EditorInput, {
				onChange: (v: string) => { lastValue = v; },
			}),
			tui,
		);
		await wait(30);
		await term.flush();

		term.sendInput('hello');
		await wait(30);
		await term.flush();

		expect(lastValue).toBe('hello');
		inst.unmount();
		tui.stop();
	});

	it('handles onSubmit', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		let submitted = '';
		const inst = render(
			React.createElement(EditorInput, {
				onSubmit: (v: string) => { submitted = v; },
			}),
			tui,
		);
		await wait(30);

		term.sendInput('test');
		await wait(10);
		term.sendInput('\r');
		await wait(30);
		await term.flush();

		expect(submitted).toBe('test');
		inst.unmount();
		tui.stop();
	});

	it('respects controlled value prop', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		const inst = render(
			React.createElement(EditorInput, { value: 'controlled' }),
			tui,
		);
		await wait(30);
		await term.flush();

		const frame = term.getLastFrame();
		expect(frame!.viewport.some(l => l.includes('controlled'))).toBe(true);

		inst.unmount();
		tui.stop();
	});

	it('respects isActive=false', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		let changed = false;
		const inst = render(
			React.createElement(EditorInput, {
				isActive: false,
				onChange: () => { changed = true; },
			}),
			tui,
		);
		await wait(30);

		term.sendInput('x');
		await wait(30);

		expect(changed).toBe(false);
		inst.unmount();
		tui.stop();
	});

	it('handles disableSubmit', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		let submitted = false;
		const inst = render(
			React.createElement(EditorInput, {
				disableSubmit: true,
				onSubmit: () => { submitted = true; },
			}),
			tui,
		);
		await wait(30);

		term.sendInput('test');
		term.sendInput('\r');
		await wait(30);

		expect(submitted).toBe(false);
		inst.unmount();
		tui.stop();
	});

	it('renders with lineNumbers', async () => {
		const { EditorInput } = await import('../src/components/EditorInput.js');
		const term = new TestTerminal(60, 15);
		const tui = new TUI(term);
		tui.start();

		// GIVEN an EditorInput with lineNumbers enabled
		const inst = render(
			React.createElement(EditorInput, { value: 'line1', lineNumbers: true }),
			tui,
		);
		await wait(30);
		await term.flush();

		// THEN the content should be rendered with a line number prefix
		const frame = term.getLastFrame();
		expect(frame!.viewport.some(l => l.includes('1') && l.includes('line1'))).toBe(true);

		inst.unmount();
		tui.stop();
	});
});

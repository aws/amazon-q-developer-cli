import { describe, it, expect, afterEach } from 'vitest';
import { TestTerminal } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';
import { SelectList, type SelectItem } from '../src/components/SelectList.js';

const wait = (ms = 10) => new Promise(r => setTimeout(r, ms));

const items: SelectItem[] = [
	{ value: 'apple', label: 'Apple', description: 'A red fruit' },
	{ value: 'banana', label: 'Banana', description: 'A yellow fruit' },
	{ value: 'cherry', label: 'Cherry', description: 'A small fruit' },
	{ value: 'date', label: 'Date' },
	{ value: 'elderberry', label: 'Elderberry' },
];

describe('Select E2E', () => {
	let term: TestTerminal;
	let tui: TUI;

	afterEach(() => {
		tui?.stop();
	});

	function setup(cols = 60, rows = 10) {
		term = new TestTerminal(cols, rows);
		tui = new TUI(term);
	}

	it('should render list with selected indicator', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.start();

		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport[0]).toContain('Apple');
		expect(frame.viewport[0]).toContain('→');
	});

	it('should navigate and update selection', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.setFocus(list);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('\x1b[B'); // down
		tui.requestRender();
		await wait();
		await term.flush();

		expect(list.getSelectedItem()?.value).toBe('banana');
		const frame = term.getLastFrame()!;
		expect(frame.viewport.some(l => l.includes('Banana') && l.includes('→'))).toBe(true);
	});

	it('should fire onSelect on enter', async () => {
		setup();
		const list = new SelectList(items, 3);
		let selected: SelectItem | null = null;
		list.onSelect = (item) => { selected = item; };
		tui.addChild(list);
		tui.setFocus(list);
		tui.start();

		term.sendInput('\x1b[B'); // down to banana
		term.sendInput('\r'); // enter

		expect(selected?.value).toBe('banana');
	});

	it('should show scroll indicator', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.start();

		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		// Should show (1/5) or similar
		expect(frame.viewport.some(l => l.includes('/'))).toBe(true);
	});

	it('should filter items', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.start();

		list.setFilter('ch');
		tui.requestRender();
		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport.some(l => l.includes('Cherry'))).toBe(true);
		expect(frame.viewport.some(l => l.includes('Apple'))).toBe(false);
	});

	it('should show descriptions', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.start();

		await wait();
		await term.flush();

		const frame = term.getLastFrame()!;
		expect(frame.viewport[0]).toContain('red fruit');
	});

	it('should wrap around navigation', async () => {
		setup();
		const list = new SelectList(items, 3);
		tui.addChild(list);
		tui.setFocus(list);
		tui.start();

		// Go up from first item
		term.sendInput('\x1b[A'); // up
		expect(list.getSelectedItem()?.value).toBe('elderberry');
	});
});

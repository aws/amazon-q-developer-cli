import { describe, it, expect, beforeEach } from 'vitest';
import { SelectList, type SelectItem } from '../src/components/SelectList.js';

const items: SelectItem[] = [
	{ value: 'apple', label: 'Apple', description: 'A fruit' },
	{ value: 'banana', label: 'Banana', description: 'Yellow fruit' },
	{ value: 'cherry', label: 'Cherry' },
	{ value: 'date', label: 'Date', description: 'Sweet fruit' },
	{ value: 'elderberry', label: 'Elderberry' },
];

describe('SelectList', () => {
	let list: SelectList;

	beforeEach(() => {
		list = new SelectList(items, 3);
	});

	it('should render visible items', () => {
		const lines = list.render(60);
		expect(lines.length).toBeGreaterThanOrEqual(3);
		expect(lines[0]).toContain('Apple');
	});

	it('should show selected indicator', () => {
		const lines = list.render(60);
		expect(lines[0]).toContain('→');
	});

	it('should navigate down', () => {
		list.handleInput('\x1b[B'); // down arrow
		expect(list.getSelectedItem()?.value).toBe('banana');
	});

	it('should navigate up', () => {
		list.handleInput('\x1b[B'); // down
		list.handleInput('\x1b[A'); // up
		expect(list.getSelectedItem()?.value).toBe('apple');
	});

	it('should wrap around at bottom', () => {
		for (let i = 0; i < items.length; i++) {
			list.handleInput('\x1b[B'); // down
		}
		expect(list.getSelectedItem()?.value).toBe('apple');
	});

	it('should wrap around at top', () => {
		list.handleInput('\x1b[A'); // up from first
		expect(list.getSelectedItem()?.value).toBe('elderberry');
	});

	it('should fire onSelect on enter', () => {
		let selected: SelectItem | null = null;
		list.onSelect = (item) => { selected = item; };
		list.handleInput('\x1b[B'); // down to banana
		list.handleInput('\r'); // enter
		expect(selected?.value).toBe('banana');
	});

	it('should fire onCancel on escape', () => {
		let cancelled = false;
		list.onCancel = () => { cancelled = true; };
		list.handleInput('\x1b'); // escape
		expect(cancelled).toBe(true);
	});

	it('should filter items', () => {
		list.setFilter('ch');
		expect(list.getSelectedItem()?.value).toBe('cherry');
		const lines = list.render(60);
		expect(lines.some(l => l.includes('Cherry'))).toBe(true);
		expect(lines.some(l => l.includes('Apple'))).toBe(false);
	});

	it('should show no match message', () => {
		list.setFilter('zzz');
		const lines = list.render(60);
		expect(lines[0]).toContain('No matching');
	});

	it('should show scroll indicator', () => {
		const lines = list.render(60);
		// 5 items, maxVisible=3, should show scroll info
		expect(lines.some(l => l.includes('/'))).toBe(true);
	});

	it('should show descriptions', () => {
		const lines = list.render(60);
		expect(lines[0]).toContain('A fruit');
	});

	it('should fire onSelectionChange', () => {
		const changes: string[] = [];
		list.onSelectionChange = (item) => { changes.push(item.value); };
		list.handleInput('\x1b[B'); // down
		list.handleInput('\x1b[B'); // down
		expect(changes).toEqual(['banana', 'cherry']);
	});
});

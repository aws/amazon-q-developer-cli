import { describe, it, expect } from 'vitest';
import { sliceByColumn, sliceWithWidth, truncateToWidth } from '../src/utils/slice.js';
import { visibleWidth } from '../src/utils/visible-width.js';

describe('sliceByColumn', () => {
	it('slices ASCII text', () => {
		expect(sliceByColumn('hello world', 0, 5)).toBe('hello');
		expect(sliceByColumn('hello world', 6, 5)).toBe('world');
	});

	it('slices with ANSI codes', () => {
		const text = '\x1b[31mhello\x1b[0m world';
		const result = sliceByColumn(text, 0, 5);
		expect(visibleWidth(result)).toBe(5);
	});

	it('handles CJK at boundaries', () => {
		// 你(2) 好(2) = 4 cols
		const result = sliceByColumn('你好世', 0, 4);
		expect(visibleWidth(result)).toBe(4);
	});

	it('returns empty for zero length', () => {
		expect(sliceByColumn('hello', 0, 0)).toBe('');
	});
});

describe('sliceWithWidth', () => {
	it('returns text and width', () => {
		const result = sliceWithWidth('hello', 0, 3);
		expect(result.text).toBe('hel');
		expect(result.width).toBe(3);
	});
});

describe('truncateToWidth', () => {
	it('returns text unchanged if within width', () => {
		expect(truncateToWidth('hello', 10)).toBe('hello');
	});

	it('truncates with ellipsis', () => {
		const result = truncateToWidth('hello world', 8);
		expect(visibleWidth(result)).toBeLessThanOrEqual(8);
		expect(result).toContain('...');
	});

	it('pads when requested', () => {
		const result = truncateToWidth('hi', 10, '...', true);
		expect(visibleWidth(result)).toBe(10);
	});

	it('handles ANSI codes', () => {
		const result = truncateToWidth('\x1b[31mhello world\x1b[0m', 8);
		expect(result).toContain('...');
		expect(result).toContain('\x1b[0m'); // reset before ellipsis
	});
});

import { describe, it, expect } from 'vitest';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';

describe('wrapTextWithAnsi', () => {
	it('returns single line for short text', () => {
		expect(wrapTextWithAnsi('hello', 80)).toEqual(['hello']);
	});

	it('returns empty line for empty string', () => {
		expect(wrapTextWithAnsi('', 80)).toEqual(['']);
	});

	it('wraps at word boundary', () => {
		const result = wrapTextWithAnsi('hello world', 6);
		expect(result).toEqual(['hello', 'world']);
	});

	it('breaks long words', () => {
		const result = wrapTextWithAnsi('abcdefghij', 5);
		expect(result).toEqual(['abcde', 'fghij']);
	});

	it('preserves newlines', () => {
		const result = wrapTextWithAnsi('line1\nline2', 80);
		expect(result).toEqual(['line1', 'line2']);
	});

	it('preserves ANSI codes across wraps', () => {
		const result = wrapTextWithAnsi('\x1b[31mhello world\x1b[0m', 6);
		expect(result.length).toBe(2);
		// First line should have the red code
		expect(result[0]).toContain('\x1b[31m');
		// Second line should re-apply red
		expect(result[1]).toContain('\x1b[');
	});

	it('trims trailing whitespace from wrapped lines', () => {
		const result = wrapTextWithAnsi('hello   world', 8);
		for (const line of result) {
			expect(line).toBe(line.trimEnd());
		}
	});

	it('handles CJK characters in wrapping', () => {
		// Each CJK char is width 2, so 3 chars = width 6
		const result = wrapTextWithAnsi('你好世界', 5);
		// Should wrap: 你好 (4) then 世界 (4)
		expect(result.length).toBe(2);
	});
});

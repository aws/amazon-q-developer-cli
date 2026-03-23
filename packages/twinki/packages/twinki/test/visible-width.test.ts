import { describe, it, expect } from 'vitest';
import { visibleWidth } from '../src/utils/visible-width.js';

describe('visibleWidth', () => {
	it('returns 0 for empty string', () => {
		expect(visibleWidth('')).toBe(0);
	});

	it('returns length for pure ASCII', () => {
		expect(visibleWidth('hello')).toBe(5);
		expect(visibleWidth('abc123!@#')).toBe(9);
	});

	it('handles CJK characters as width 2', () => {
		expect(visibleWidth('你好')).toBe(4);
		expect(visibleWidth('日本語')).toBe(6);
		expect(visibleWidth('a你b')).toBe(4);
	});

	it('handles emoji as width 2', () => {
		expect(visibleWidth('🎉')).toBe(2);
		expect(visibleWidth('👍')).toBe(2);
	});

	it('handles multi-codepoint emoji', () => {
		expect(visibleWidth('👨‍👩‍👧‍👦')).toBe(2);
		expect(visibleWidth('🏳️‍🌈')).toBe(2);
	});

	it('strips ANSI SGR codes', () => {
		expect(visibleWidth('\x1b[31mred\x1b[0m')).toBe(3);
		expect(visibleWidth('\x1b[1;32mbold green\x1b[0m')).toBe(10);
	});

	it('strips cursor codes', () => {
		expect(visibleWidth('\x1b[2Kline')).toBe(4);
		expect(visibleWidth('\x1b[Hhome')).toBe(4);
	});

	it('strips OSC 8 hyperlinks', () => {
		expect(visibleWidth('\x1b]8;;https://example.com\x07link\x1b]8;;\x07')).toBe(4);
	});

	it('converts tabs to 3 spaces', () => {
		expect(visibleWidth('\t')).toBe(3);
		expect(visibleWidth('a\tb')).toBe(5);
	});

	it('handles mixed content', () => {
		expect(visibleWidth('\x1b[31m你好\x1b[0m world')).toBe(10);
	});

	describe('two-tier cache', () => {
		it('returns consistent results for short strings (grapheme cache)', () => {
			const short = '\x1b[38;2;255;0;0mx\x1b[0m';
			expect(short.length).toBeLessThanOrEqual(20);
			expect(visibleWidth(short)).toBe(1);
			expect(visibleWidth(short)).toBe(1);
		});

		it('returns consistent results for long strings (line cache)', () => {
			const long = '\x1b[38;2;255;0;0m' + 'x'.repeat(20) + '\x1b[0m';
			expect(long.length).toBeGreaterThan(20);
			expect(visibleWidth(long)).toBe(20);
			expect(visibleWidth(long)).toBe(20);
		});
	});

	describe('parity with string-width', () => {
		const cases: [string, number][] = [
			['hello world', 11],
			['', 0],
			[' ', 1],
			['~', 1],
			['\x1b[31mred\x1b[0m', 3],
			['\x1b[0m', 0],
			['\x1b[1;31;42mbold red on green\x1b[0m', 17],
			['\x1b[38;5;196m256color\x1b[0m', 8],
			['\x1b[38;2;255;128;0mtruecolor\x1b[0m', 9],
			['\x1b[2Kline', 4],
			['\x1b[Hhome', 4],
			['🚀', 2],
			['hello 🌍', 8],
			['你好', 4],
			['\x1b[31m你好\x1b[0m', 4],
			['\t', 3],
			['a\tb', 5],
			['\x1b[31mhello\x1b[0m \x1b[32mworld\x1b[0m', 11],
		];

		for (const [input, expected] of cases) {
			const label = JSON.stringify(input).slice(0, 60);
			it(`${label} → ${expected}`, () => {
				expect(visibleWidth(input)).toBe(expected);
			});
		}
	});
});

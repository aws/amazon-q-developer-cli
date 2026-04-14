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

	describe('Bun.stringWidth Unicode coverage', () => {
		const unicodeCases: [string, string, number][] = [
			['ZWJ family', '👨‍👩‍👧‍👦', 2],
			['flag', '🇺🇸', 2],
			['skin tone', '👋🏽', 2],
			['keycap 1', '1️⃣', 2],
			['rainbow flag', '🏳️‍🌈', 2],
			['VS16 smiley', '☺️', 2],
			['text smiley', '☺', 1],
			['copyright', '©', 1],
			['registered', '®', 1],
			['trademark', '™', 1],
			['fullwidth A', 'Ａ', 2],
			['halfwidth katakana', 'ｱ', 1],
			['combining acute', 'e\u0301', 1],
			['combining double', 'a\u0300\u0301', 1],
			['ZWJ alone', '\u200D', 0],
			['ZWSP', '\u200B', 0],
			['ZWNJ', '\u200C', 0],
			['BOM', '\uFEFF', 0],
			['bullet', '•', 1],
			['ellipsis', '…', 1],
			['em dash', '—', 1],
			['musical symbol', '𝄞', 1],
			['OSC8 link', '\x1b]8;;https://x.com\x07link\x1b]8;;\x07', 4],
			['mixed emoji+CJK', 'hello 🌍 世界', 13],
		];

		for (const [label, input, expected] of unicodeCases) {
			it(`${label} → ${expected}`, () => {
				expect(visibleWidth(input)).toBe(expected);
			});
		}
	});

	describe('fastStringWidth parity with string-width@8', () => {
		// These 60 cases cover ASCII, ANSI, CJK, emoji (simple, ZWJ, flags,
		// skin tones, keycaps, VS16), combining marks, zero-width chars,
		// fullwidth/halfwidth, OSC8 links, and mixed content.
		const parityCases: [string, string, number][] = [
			['ASCII', 'hello', 5],
			['empty', '', 0],
			['space', ' ', 1],
			['tilde', '~', 1],
			['ANSI red', '\x1b[31mred\x1b[0m', 3],
			['ANSI reset only', '\x1b[0m', 0],
			['ANSI bold+color+bg', '\x1b[1;31;42mbold red on green\x1b[0m', 17],
			['ANSI 256-color', '\x1b[38;5;196m256color\x1b[0m', 8],
			['ANSI truecolor', '\x1b[38;2;255;128;0mtruecolor\x1b[0m', 9],
			['ANSI erase line', '\x1b[2Kline', 4],
			['ANSI cursor home', '\x1b[Hhome', 4],
			['rocket emoji', '\u{1F680}', 2],
			['hello + globe', 'hello \u{1F30D}', 8],
			['CJK Chinese', '\u4F60\u597D', 4],
			['CJK Japanese', '\u65E5\u672C\u8A9E', 6],
			['CJK Korean', '\uD55C\uAD6D\uC5B4', 6],
			['ANSI + CJK', '\x1b[31m\u4F60\u597D\x1b[0m', 4],
			['tab', '\t', 3],
			['a + tab + b', 'a\tb', 5],
			['two ANSI spans', '\x1b[31mhello\x1b[0m \x1b[32mworld\x1b[0m', 11],
			['ZWJ family', '\u{1F468}\u200D\u{1F469}\u200D\u{1F467}\u200D\u{1F466}', 2],
			['flag US', '\u{1F1FA}\u{1F1F8}', 2],
			['skin tone wave', '\u{1F44B}\u{1F3FD}', 2],
			['keycap 1', '1\uFE0F\u20E3', 2],
			['keycap #', '#\uFE0F\u20E3', 2],
			['rainbow flag', '\u{1F3F3}\uFE0F\u200D\u{1F308}', 2],
			['trans flag', '\u{1F3F3}\uFE0F\u200D\u26A7\uFE0F', 2],
			['person bald', '\u{1F9D1}\u200D\u{1F9B2}', 2],
			['VS16 smiley', '\u263A\uFE0F', 2],
			['text smiley (no VS16)', '\u263A', 1],
			['star', '\u2B50', 2],
			['warning + VS16', '\u26A0\uFE0F', 2],
			['hourglass', '\u23F3', 2],
			['watch', '\u231A', 2],
			['copyright', '\u00A9', 1],
			['registered', '\u00AE', 1],
			['trademark', '\u2122', 1],
			['fullwidth A', '\uFF21', 2],
			['halfwidth katakana', '\uFF71', 1],
			['combining acute', 'e\u0301', 1],
			['combining double', 'a\u0300\u0301', 1],
			['ZWJ alone', '\u200D', 0],
			['ZWSP', '\u200B', 0],
			['ZWNJ', '\u200C', 0],
			['BOM', '\uFEFF', 0],
			['null', '\x00', 0],
			['bell', '\x07', 0],
			['bullet', '\u2022', 1],
			['ellipsis', '\u2026', 1],
			['em dash', '\u2014', 1],
			['musical symbol', '\u{1D11E}', 1],
			['math bold A', '\u{1D400}', 1],
			['OSC8 link', '\x1b]8;;https://x.com\x07link\x1b]8;;\x07', 4],
			['mixed emoji+CJK', 'hello \u{1F30D} \u4E16\u754C', 13],
			['ANSI + emoji', '\x1b[31m\u{1F680}\x1b[0m go', 5],
			['emoji row', '\u{1F389}\u{1F38A}\u{1F388}\u{1F381}', 8],
			['radioactive', '\u2622', 1],
			['biohazard', '\u2623', 1],
			['orthodox cross', '\u2626', 1],
			['star and crescent', '\u262A', 1],
		];

		for (const [label, input, expected] of parityCases) {
			it(`${label} → ${expected}`, () => {
				expect(visibleWidth(input)).toBe(expected);
			});
		}
	});
});

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
		expect(visibleWidth('a你b')).toBe(4); // 1 + 2 + 1
	});

	it('handles emoji as width 2', () => {
		expect(visibleWidth('🎉')).toBe(2);
		expect(visibleWidth('👍')).toBe(2);
	});

	it('handles multi-codepoint emoji', () => {
		expect(visibleWidth('👨‍👩‍👧‍👦')).toBe(2); // family emoji
		expect(visibleWidth('🏳️‍🌈')).toBe(2); // rainbow flag
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
		expect(visibleWidth('a\tb')).toBe(5); // 1 + 3 + 1
	});

	it('handles mixed content', () => {
		expect(visibleWidth('\x1b[31m你好\x1b[0m world')).toBe(10); // 4 + 1 + 5
	});
});

import { describe, it, expect } from 'vitest';
import { AnsiCodeTracker, extractAnsiCode } from '../src/utils/ansi.js';

describe('extractAnsiCode', () => {
	it('returns null for non-escape', () => {
		expect(extractAnsiCode('hello', 0)).toBeNull();
	});

	it('extracts SGR codes', () => {
		const result = extractAnsiCode('\x1b[31m', 0);
		expect(result).toEqual({ code: '\x1b[31m', length: 5 });
	});

	it('extracts complex SGR codes', () => {
		const result = extractAnsiCode('\x1b[1;38;2;255;0;0m', 0);
		expect(result).toEqual({ code: '\x1b[1;38;2;255;0;0m', length: 17 });
	});

	it('extracts cursor codes', () => {
		expect(extractAnsiCode('\x1b[2K', 0)).toEqual({ code: '\x1b[2K', length: 4 });
		expect(extractAnsiCode('\x1b[H', 0)).toEqual({ code: '\x1b[H', length: 3 });
	});

	it('extracts OSC sequences', () => {
		const osc = '\x1b]8;;https://example.com\x07';
		expect(extractAnsiCode(osc, 0)).toEqual({ code: osc, length: osc.length });
	});

	it('extracts at offset', () => {
		const str = 'abc\x1b[31mdef';
		expect(extractAnsiCode(str, 3)).toEqual({ code: '\x1b[31m', length: 5 });
	});
});

describe('AnsiCodeTracker', () => {
	it('tracks bold', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1m');
		expect(t.getActiveCodes()).toBe('\x1b[1m');
	});

	it('tracks foreground color', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[31m');
		expect(t.getActiveCodes()).toBe('\x1b[31m');
	});

	it('tracks 256-color', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[38;5;196m');
		expect(t.getActiveCodes()).toBe('\x1b[38;5;196m');
	});

	it('tracks RGB color', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[38;2;255;0;0m');
		expect(t.getActiveCodes()).toBe('\x1b[38;2;255;0;0m');
	});

	it('resets on \\x1b[0m', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1m');
		t.process('\x1b[31m');
		t.process('\x1b[0m');
		expect(t.getActiveCodes()).toBe('');
	});

	it('resets on \\x1b[m', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1m');
		t.process('\x1b[m');
		expect(t.getActiveCodes()).toBe('');
	});

	it('tracks multiple attributes', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1;3;31m'); // bold + italic + red
		expect(t.hasActiveCodes()).toBe(true);
		const codes = t.getActiveCodes();
		expect(codes).toContain('1');
		expect(codes).toContain('3');
		expect(codes).toContain('31');
	});

	it('getLineEndReset returns underline reset when active', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[4m');
		expect(t.getLineEndReset()).toBe('\x1b[24m');
	});

	it('getLineEndReset returns empty when no underline', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1m');
		expect(t.getLineEndReset()).toBe('');
	});

	it('clear resets all state', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1;31m');
		t.clear();
		expect(t.hasActiveCodes()).toBe(false);
	});
});

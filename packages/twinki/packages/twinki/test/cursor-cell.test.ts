import { describe, it, expect } from 'vitest';
import { uninvertCursorCell } from '../src/renderer/cursor-cell.js';

const INV = '\x1b[7m';
const NOINV = '\x1b[27m';

describe('uninvertCursorCell', () => {
	it('un-inverts a cell whose inverse opens at the cell', () => {
		const line = `ab${INV}X${NOINV}cd`;
		expect(uninvertCursorCell(line, 2)).toBe(
			`ab${INV}${NOINV}X${INV}${NOINV}cd`
		);
	});

	it('un-inverts a cell inheriting inverse from earlier in the line', () => {
		const line = `${INV}abc`;
		expect(uninvertCursorCell(line, INV.length + 1)).toBe(
			`${INV}a${NOINV}b${INV}c`
		);
	});

	it('leaves a non-inverse cell untouched', () => {
		const line = 'abc';
		expect(uninvertCursorCell(line, 1)).toBe('abc');
	});

	it('treats SGR reset as closing inverse', () => {
		const line = `${INV}a\x1b[0mbc`;
		expect(uninvertCursorCell(line, `${INV}a\x1b[0m`.length)).toBe(line);
	});

	it('treats SGR 27 as closing inverse', () => {
		const line = `${INV}a${NOINV}bc`;
		expect(uninvertCursorCell(line, `${INV}a${NOINV}`.length)).toBe(line);
	});

	it('does not mistake extended-color params for inverse', () => {
		// Palette color 7 (38;5;7) and RGB with a 7 component are not SGR 7.
		const palette = '\x1b[38;5;7mabc';
		expect(uninvertCursorCell(palette, '\x1b[38;5;7m'.length)).toBe(palette);
		const rgb = '\x1b[48;2;7;7;7mabc';
		expect(uninvertCursorCell(rgb, '\x1b[48;2;7;7;7m'.length)).toBe(rgb);
	});

	it('handles inverse combined with other attributes in one sequence', () => {
		const line = `\x1b[1;7mX${NOINV}rest`;
		expect(uninvertCursorCell(line, 0)).toBe(
			`\x1b[1;7m${NOINV}X${INV}${NOINV}rest`
		);
	});

	it('skips escape sequences between idx and the cell', () => {
		// Marker position may be followed by SGRs before the visible char.
		const line = `ab\x1b[35m${INV}X${NOINV}cd`;
		expect(uninvertCursorCell(line, 2)).toBe(
			`ab\x1b[35m${INV}${NOINV}X${INV}${NOINV}cd`
		);
	});

	it('wraps a full grapheme, not a code unit', () => {
		const line = `${INV}👩‍👩‍👧rest`;
		expect(uninvertCursorCell(line, INV.length)).toBe(
			`${INV}${NOINV}👩‍👩‍👧${INV}rest`
		);
	});

	it('is a no-op when nothing follows the cursor position', () => {
		const line = `abc${INV}`;
		expect(uninvertCursorCell(line, line.length)).toBe(line);
	});

	it('is a no-op on an empty line', () => {
		expect(uninvertCursorCell('', 0)).toBe('');
	});
});

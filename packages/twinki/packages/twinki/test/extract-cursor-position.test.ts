import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import { CURSOR_MARKER } from '../src/renderer/component.js';
import { TestTerminal } from './helpers.js';

/** Access the private method via `as any`. */
function extractCursorPosition(
	tui: TUI,
	lines: string[],
	height: number
): { row: number; col: number } | null {
	return (tui as any).extractCursorPosition(lines, height);
}

function makeTUI(): TUI {
	const term = new TestTerminal(80, 24);
	return new TUI(term);
}

describe('extractCursorPosition', () => {
	it('returns null when no marker is present', () => {
		const tui = makeTUI();
		const lines = ['hello', 'world'];
		expect(extractCursorPosition(tui, lines, 10)).toBeNull();
	});

	it('finds marker in viewport and returns position', () => {
		const tui = makeTUI();
		const lines = ['hello', `ab${CURSOR_MARKER}cd`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 1, col: 2 });
	});

	it('strips marker from the line when found in viewport', () => {
		const tui = makeTUI();
		const lines = ['hello', `ab${CURSOR_MARKER}cd`];
		extractCursorPosition(tui, lines, 10);
		expect(lines[1]).toBe('abcd');
	});

	it('returns col based on visible width (not byte offset)', () => {
		const tui = makeTUI();
		// CJK char is 2 columns wide
		const lines = [`你${CURSOR_MARKER}好`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 2 });
	});

	it('finds marker at start of line (col 0)', () => {
		const tui = makeTUI();
		const lines = [`${CURSOR_MARKER}hello`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 0 });
		expect(lines[0]).toBe('hello');
	});

	it('finds marker at end of line', () => {
		const tui = makeTUI();
		const lines = [`hello${CURSOR_MARKER}`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 5 });
		expect(lines[0]).toBe('hello');
	});

	it('scans bottom-up and returns last marker in viewport', () => {
		const tui = makeTUI();
		// Only one marker exists in practice, but the scan goes bottom-up
		const lines = ['line0', 'line1', `x${CURSOR_MARKER}y`, 'line3'];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 2, col: 1 });
	});

	it('strips marker above viewport and returns null (tmux fix)', () => {
		const tui = makeTUI();
		// 5 lines, viewport height 3 → viewportTop = 2, viewport = lines[2..4]
		const lines = [
			`stale${CURSOR_MARKER}marker`,
			'line1',
			'line2',
			'line3',
			'line4',
		];
		const result = extractCursorPosition(tui, lines, 3);
		expect(result).toBeNull();
		expect(lines[0]).toBe('stalemarker');
	});

	it('does not strip lines in viewport when marker is above', () => {
		const tui = makeTUI();
		const lines = [
			`has${CURSOR_MARKER}marker`,
			'clean1',
			'clean2',
		];
		// height=2 → viewportTop=1, viewport = lines[1..2]
		const result = extractCursorPosition(tui, lines, 2);
		expect(result).toBeNull();
		expect(lines[0]).toBe('hasmarker');
		expect(lines[1]).toBe('clean1');
		expect(lines[2]).toBe('clean2');
	});

	it('handles all lines fitting in viewport (no above-viewport region)', () => {
		const tui = makeTUI();
		const lines = [`a${CURSOR_MARKER}b`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 1 });
		expect(lines[0]).toBe('ab');
	});

	it('handles empty lines array', () => {
		const tui = makeTUI();
		const lines: string[] = [];
		expect(extractCursorPosition(tui, lines, 10)).toBeNull();
	});
});

describe('extractCursorPosition inverse suppression', () => {
	const INV = '\x1b[7m';
	const NOINV = '\x1b[27m';

	function makeTUIWithVisibleCursor(): TUI {
		const term = new TestTerminal(80, 24);
		return new TUI(term, true);
	}

	it('un-inverts the marker cell when the hardware cursor is visible', () => {
		const tui = makeTUIWithVisibleCursor();
		const lines = [`ab${CURSOR_MARKER}${INV}X${NOINV}cd`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 2 });
		expect(lines[0]).toBe(`ab${INV}${NOINV}X${INV}${NOINV}cd`);
	});

	it('leaves the marker cell inverse when the hardware cursor is hidden', () => {
		const tui = makeTUI();
		const lines = [`ab${CURSOR_MARKER}${INV}X${NOINV}cd`];
		const result = extractCursorPosition(tui, lines, 10);
		expect(result).toEqual({ row: 0, col: 2 });
		expect(lines[0]).toBe(`ab${INV}X${NOINV}cd`);
	});

	it('leaves a non-inverse marker cell untouched', () => {
		const tui = makeTUIWithVisibleCursor();
		const lines = [`ab${CURSOR_MARKER}cd`];
		extractCursorPosition(tui, lines, 10);
		expect(lines[0]).toBe('abcd');
	});

	it('un-inverts inherited inversion on the marker cell', () => {
		const tui = makeTUIWithVisibleCursor();
		const lines = [`${INV}selected ${CURSOR_MARKER}row`];
		extractCursorPosition(tui, lines, 10);
		expect(lines[0]).toBe(`${INV}selected ${NOINV}r${INV}ow`);
	});

	it('does not touch inverse cells without the marker', () => {
		const tui = makeTUIWithVisibleCursor();
		const lines = [`${INV}X${NOINV}`, `ab${CURSOR_MARKER}cd`];
		extractCursorPosition(tui, lines, 10);
		expect(lines[0]).toBe(`${INV}X${NOINV}`);
	});
});

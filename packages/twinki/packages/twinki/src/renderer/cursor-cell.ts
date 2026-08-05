import { extractAnsiCode, AnsiCodeTracker } from '../utils/ansi.js';

const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/**
 * Removes the inverse attribute from the single grapheme at `idx`, leaving
 * every other attribute intact.
 *
 * Applied to the cell the visible hardware cursor occupies: the terminal
 * already inverts that cell, so a software inverse painted there cancels out
 * and the cursor disappears. The inversion may be opened at the cell itself or
 * inherited from an SGR earlier in the line; both are handled by tracking the
 * SGR state up to the cell.
 */
export function uninvertCursorCell(line: string, idx: number): string {
	const tracker = new AnsiCodeTracker();
	let i = 0;
	while (i < line.length) {
		const seq = extractAnsiCode(line, i);
		if (seq) {
			tracker.process(seq.code);
			i += seq.length;
			continue;
		}
		if (i >= idx) break;
		i++;
	}
	if (!tracker.isInverse || i >= line.length) return line;

	const first = segmenter.segment(line.slice(i))[Symbol.iterator]().next();
	if (first.done) return line;
	const grapheme = first.value.segment;

	return (
		line.slice(0, i) +
		'\x1b[27m' +
		grapheme +
		'\x1b[7m' +
		line.slice(i + grapheme.length)
	);
}

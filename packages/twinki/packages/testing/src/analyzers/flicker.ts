import type { Frame } from '../frame-capturing-terminal.js';

export interface FlickerEvent {
	frameIndex: number;
	row: number;
	col: number;
	prevChar: string;
	nextChar: string;
}

export interface FlickerReport {
	events: FlickerEvent[];
	clean: boolean;
}

export function analyzeFlicker(frames: Frame[], blankChar = ' '): FlickerReport {
	const events: FlickerEvent[] = [];

	for (let i = 1; i < frames.length - 1; i++) {
		const prev = frames[i - 1]!;
		const curr = frames[i]!;
		const next = frames[i + 1]!;

		const maxRows = Math.max(prev.viewport.length, curr.viewport.length, next.viewport.length);
		for (let row = 0; row < maxRows; row++) {
			const prevLine = prev.viewport[row] ?? '';
			const currLine = curr.viewport[row] ?? '';
			const nextLine = next.viewport[row] ?? '';
			const maxCols = Math.max(prevLine.length, currLine.length, nextLine.length);

			for (let col = 0; col < maxCols; col++) {
				const p = prevLine[col] ?? blankChar;
				const c = currLine[col] ?? blankChar;
				const n = nextLine[col] ?? blankChar;

				if (p !== blankChar && c === blankChar && n !== blankChar) {
					events.push({ frameIndex: i, row, col, prevChar: p, nextChar: n });
				}
			}
		}
	}

	return { events, clean: events.length === 0 };
}
